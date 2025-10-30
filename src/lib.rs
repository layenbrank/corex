pub mod bootstrap {
    pub mod controller;
    pub mod service;
}

pub mod schedule {
    pub mod controller;
    pub mod service;
}

pub mod copy {
    pub mod controller;
    pub mod service;
}

pub mod scrub {
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
    pub mod scan;
    pub mod verifier;
}
