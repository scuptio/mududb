use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComponentTarget {
    #[default]
    P2,
    P3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTarget {
    #[default]
    P1,
    Component(ComponentTarget),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeOpt {
    pub target: RuntimeTarget,
    pub enable_async: bool,
}

impl RuntimeOpt {
    pub fn from_legacy_enable_p2(enable_p2: bool, enable_async: bool) -> Self {
        let target = if enable_p2 {
            RuntimeTarget::Component(ComponentTarget::P2)
        } else {
            RuntimeTarget::P1
        };
        Self {
            target,
            enable_async,
        }
    }

    pub fn uses_component_model(&self) -> bool {
        self.target.uses_component_model()
    }

    pub fn component_target(&self) -> Option<ComponentTarget> {
        self.target.component_target()
    }
}

impl RuntimeTarget {
    pub fn uses_component_model(self) -> bool {
        matches!(self, Self::Component(_))
    }

    pub fn component_target(self) -> Option<ComponentTarget> {
        match self {
            Self::P1 => None,
            Self::Component(target) => Some(target),
        }
    }
}

impl Default for RuntimeOpt {
    fn default() -> Self {
        Self {
            target: RuntimeTarget::P1,
            enable_async: false,
        }
    }
}
