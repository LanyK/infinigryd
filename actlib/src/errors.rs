use core::fmt::Debug;

/// Any Error that can occur when using the *actlib* library.
#[derive(Debug)]
pub enum ActlibError {
    ActorNotFound(String),
    LockPoisoned(String),
    SpawnFailed(String),
    InvalidState(String),
    NetworkError(String),
    InvalidActorRef(String),
}

impl ActlibError {
    pub(crate) fn from_poison_error<T: Debug>(e: &std::sync::PoisonError<T>) -> ActlibError {
        ActlibError::LockPoisoned(format!("{:?}", e))
    }
}

/// This macro uses the appropriate macro (specified by a shorthand as first argument) from the log-crate to notify the user about potentially dangerous behaviour.
///
/// It is only exported, so we can use it in other modules.
#[macro_export]
macro_rules! log_err_as {
    (err, $e:expr) => {{
        error!("{:?}", $e);
    }};
    (error, $e:expr) => {{
        error!("{:?}", $e);
    }};
    (warn, $e:expr) => {{
        warn!("{:?}", $e);
    }};
    (warning, $e:expr) => {{
        warn!("{:?}", $e);
    }};
}
