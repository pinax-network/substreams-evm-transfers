//! Minimal, bounded host for the two immutable historical map binaries.
//! No stores, RPC, WASI, network, filesystem or production runtime integration.
use sha2::{Digest, Sha256};
use wasmi::{Caller, Config, Engine, Error, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Val};

pub const MAP_SHA256: &str = "5d6a71d3d6eeb1b91cef2d599b61c54f145c936c6789a5456a69c281ca5f6126";
pub const DB_SHA256: &str = "111f1d391d6840c6581cca5ffe5554456a1dc60ff7ce478bc26b0700778d3091";
const MAX_BYTES: usize = 4 * 1024 * 1024;

struct Host {
    limits: StoreLimits,
    output: Option<Vec<u8>>,
    panicked: bool,
}

pub struct Oracle {
    engine: Engine,
    map: Module,
    database: Module,
}

impl Oracle {
    pub fn new() -> Self {
        let map = include_bytes!("../fixtures/legacy-v0.4.0/erc20_transfers.wasm");
        let database = include_bytes!("../fixtures/legacy-v0.4.0/db_out.wasm");
        assert_eq!(hex::encode(Sha256::digest(map)), MAP_SHA256);
        assert_eq!(hex::encode(Sha256::digest(database)), DB_SHA256);
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let map = Module::new(&engine, map.as_slice()).unwrap();
        let database = Module::new(&engine, database.as_slice()).unwrap();
        Self { engine, map, database }
    }

    /// `None` means a registered historical package panic, never a host failure.
    pub fn map(&self, block: &[u8]) -> Option<Vec<u8>> {
        self.run(&self.map, "map_events", &[block])
    }

    pub fn database(&self, clock: &[u8], events: &[u8]) -> Vec<u8> {
        self.run(&self.database, "db_out", &[b"hex", clock, events, &[], &[]])
            .expect("db_out must not panic for valid ERC20 map output")
    }

    fn run(&self, module: &Module, function: &str, inputs: &[&[u8]]) -> Option<Vec<u8>> {
        let mut store = Store::new(
            &self.engine,
            Host {
                limits: StoreLimitsBuilder::new().memory_size(64 * 1024 * 1024).instances(1).memories(1).build(),
                output: None,
                panicked: false,
            },
        );
        store.limiter(|host| &mut host.limits);
        store.set_fuel(1_000_000_000).unwrap();
        let mut linker = Linker::<Host>::new(&self.engine);
        linker.func_wrap("logger", "println", |_pointer: i32, _length: i32| {}).unwrap();
        linker.func_wrap("env", "skip_empty_output", || {}).unwrap();
        linker
            .func_wrap(
                "env",
                "register_panic",
                |mut caller: Caller<'_, Host>, _: i32, _: i32, _: i32, _: i32, _: i32, _: i32| {
                    caller.data_mut().panicked = true;
                },
            )
            .unwrap();
        linker
            .func_wrap(
                "env",
                "output",
                |mut caller: Caller<'_, Host>, pointer: i32, length: i32| -> Result<(), Error> {
                    let pointer = usize::try_from(pointer).map_err(|_| Error::new("negative output pointer"))?;
                    let length = usize::try_from(length).map_err(|_| Error::new("negative output length"))?;
                    if length > MAX_BYTES || caller.data().output.is_some() {
                        return Err(Error::new("oversized or duplicate output"));
                    }
                    let memory = caller
                        .get_export("memory")
                        .and_then(|value| value.into_memory())
                        .ok_or_else(|| Error::new("missing memory"))?;
                    let mut bytes = vec![0; length];
                    memory.read(&caller, pointer, &mut bytes).map_err(|_| Error::new("invalid output range"))?;
                    caller.data_mut().output = Some(bytes);
                    Ok(())
                },
            )
            .unwrap();
        // Only the four declared imports above are available. New imports fail.
        let instance = linker.instantiate_and_start(&mut store, module).unwrap();
        let allocate = instance.get_typed_func::<i32, i32>(&store, "alloc").unwrap();
        let memory = instance.get_memory(&store, "memory").unwrap();
        let mut arguments = Vec::with_capacity(inputs.len() * 2);
        for bytes in inputs {
            assert!(bytes.len() <= MAX_BYTES);
            let length = i32::try_from(bytes.len()).unwrap();
            let pointer = allocate.call(&mut store, length).unwrap();
            memory.write(&mut store, usize::try_from(pointer).unwrap(), bytes).unwrap();
            arguments.extend([Val::I32(pointer), Val::I32(length)]);
        }
        let result = instance.get_func(&store, function).unwrap().call(&mut store, &arguments, &mut []);
        if store.data().panicked {
            assert!(result.is_err(), "a registered panic must trap");
            assert!(store.data().output.is_none(), "a panic must not publish output");
            None
        } else {
            result.expect("unexpected WASM host/trap/fuel failure");
            Some(store.data_mut().output.take().unwrap_or_default())
        }
        // Dropping this fresh Store releases all allocations after every module.
    }
}
