pub mod setup {
    pub mod controller;
    pub mod service;
}

pub mod task {
    pub mod controller;
    pub mod service;
}

pub mod copy {
    pub mod controller;
    pub mod service;
}

pub mod remove {
    pub mod controller;
    pub mod service;
}

pub mod generate {
    pub mod controller;
    pub mod service;
}

pub mod utils {
    pub mod error;
    pub mod file;
    pub mod ignore;
    pub mod notify;
    pub mod progress;
    pub mod verifier;
}
