pub mod bootstrap {
    pub mod schema;
    pub mod service;
}

pub mod copy {
    pub mod schema;
    pub mod service;
}

pub mod compression {
    pub mod schema;
    pub mod service;
}

pub mod scrub {
    pub mod schema;
    pub mod service;
}

pub mod schedule {
    pub mod pipeline;
    pub mod schema;
    pub mod service;
}

pub mod shade {
    pub mod schema;
    pub mod service;
}

pub mod screenshot {
    pub mod schema;
    pub mod service;
}

pub mod generate {
    pub mod schema;
    pub mod service;
}

pub mod morph {
    pub mod schema;
    pub mod service;
}

pub mod utils {
    pub mod file;
    pub mod ignore;
    pub mod notify;
    pub mod progress;
    pub mod verifier;
}
