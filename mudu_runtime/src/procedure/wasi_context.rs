use std::sync::atomic::AtomicU32;
use wasmtime_wasi::preview1::WasiP1Ctx;


pub struct ContextData {
    auto_increase: AtomicU32,
    host_mem: scc::HashMap<u32, Vec<u8>>,
}

pub struct WasiContext {
    data: ContextData,
    // .. other custom state here ..
    wasi: WasiP1Ctx,
}

impl ContextData {
    pub fn new() -> Self {
        Self {
            auto_increase: Default::default(),
            host_mem: Default::default(),
        }
    }

    pub fn add_memory(&self, vec: Vec<u8>) -> u32 {
        let mut vec = vec;
        loop {
            let n = self.auto_increase.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let result = self.host_mem.insert_sync(n, vec);
            match result {
                Ok(_) => {
                    return n
                }
                Err((_n, v)) => {
                    vec = v
                }
            }
        }
    }

    pub fn get_memory(&self, id: u32) -> Option<Vec<u8>> {
        self.host_mem.remove_sync(&id).map(|(k, v)| v)
    }
}

impl WasiContext {
    pub fn new(wasi: WasiP1Ctx) -> Self {
        WasiContext {
            data: ContextData::new(),
            wasi,
        }
    }

    pub fn context_ptr(&self) -> *const ContextData {
        &self.data
    }

    pub fn context_ref(&self) -> &ContextData {
        &self.data
    }

    pub fn wasi_mut(&mut self) -> &mut WasiP1Ctx {
        &mut self.wasi
    }
}