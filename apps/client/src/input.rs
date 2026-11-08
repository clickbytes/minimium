use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum InputContext {
    Global,
    Gameplay,
    Console,
    Map,
    Modal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputAction {
    ToggleFullscreen,
    ToggleConsole,
    ToggleMap,
    Cancel,
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    Jump,
    Interact,
    Fire,
}

impl InputAction {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "fullscreen" => Self::ToggleFullscreen,
            "console" => Self::ToggleConsole,
            "map" => Self::ToggleMap,
            "cancel" => Self::Cancel,
            "forward" => Self::MoveForward,
            "backward" => Self::MoveBackward,
            "left" => Self::MoveLeft,
            "right" => Self::MoveRight,
            "up" => Self::MoveUp,
            "down" => Self::MoveDown,
            "jump" => Self::Jump,
            "interact" => Self::Interact,
            "fire" => Self::Fire,
            _ => return None,
        })
    }
}

impl InputContext {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "global" => Self::Global,
            "gameplay" => Self::Gameplay,
            "console" => Self::Console,
            "map" => Self::Map,
            "modal" => Self::Modal,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Gameplay => "gameplay",
            Self::Console => "console",
            Self::Map => "map",
            Self::Modal => "modal",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct InputMapper {
    bindings: HashMap<(InputContext, String), InputAction>,
    #[cfg(feature = "desktop")]
    held: HashMap<String, InputAction>,
}

impl Default for InputMapper {
    fn default() -> Self {
        Self::from_setting("").expect("empty bindings are valid")
    }
}

impl InputMapper {
    pub(crate) fn from_setting(setting: &str) -> Result<Self, String> {
        let mut result = Self {
            bindings: HashMap::new(),
            #[cfg(feature = "desktop")]
            held: HashMap::new(),
        };
        for (ctx, action, key) in [
            ("global", "fullscreen", "F11"),
            ("global", "console", "F1"),
            ("global", "cancel", "Escape"),
            ("gameplay", "forward", "KeyW"),
            ("gameplay", "backward", "KeyS"),
            ("gameplay", "left", "KeyA"),
            ("gameplay", "right", "KeyD"),
            ("gameplay", "jump", "Space"),
            ("gameplay", "down", "ControlLeft"),
            ("gameplay", "down", "ControlRight"),
            ("gameplay", "interact", "KeyE"),
            ("gameplay", "map", "KeyM"),
            ("gameplay", "fire", "MouseLeft"),
        ] {
            result.insert(
                InputContext::parse(ctx).unwrap(),
                InputAction::parse(action).unwrap(),
                key.to_owned(),
            )?;
        }
        if setting.trim().is_empty() {
            return Ok(result);
        }
        for entry in setting
            .split(';')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            let (lhs, key) = entry.split_once('=').ok_or_else(|| {
                format!("invalid input binding `{entry}`; expected context.action=control")
            })?;
            let (ctx, action) = lhs.split_once('.').ok_or_else(|| {
                format!("invalid input binding `{entry}`; expected context.action=control")
            })?;
            let context =
                InputContext::parse(ctx).ok_or_else(|| format!("unknown input context `{ctx}`"))?;
            let action = InputAction::parse(action)
                .ok_or_else(|| format!("unknown input action `{action}`"))?;
            let key = key.trim();
            if key.is_empty() || key.len() > 32 || !key.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return Err(format!("invalid physical input `{key}`"));
            }
            result
                .bindings
                .retain(|(c, _), a| *c != context || *a != action);
            result.insert(context, action, key.to_owned())?;
        }
        Ok(result)
    }
    fn insert(
        &mut self,
        context: InputContext,
        action: InputAction,
        key: String,
    ) -> Result<(), String> {
        if self
            .bindings
            .iter()
            .any(|((c, k), a)| *c == context && *k == key && *a != action)
        {
            return Err(format!(
                "input `{key}` has conflicting bindings in {} context",
                context.name()
            ));
        }
        self.bindings.insert((context, key), action);
        Ok(())
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn key_down(&mut self, key: String, context: InputContext) -> Option<InputAction> {
        self.down(key, context)
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn key_up(&mut self, key: String) {
        self.held.remove(&key);
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn mouse_down(&mut self, key: String, context: InputContext) -> Option<InputAction> {
        self.down(key, context)
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn mouse_up(&mut self, key: String) {
        self.held.remove(&key);
    }
    #[cfg(feature = "desktop")]
    fn down(&mut self, key: String, context: InputContext) -> Option<InputAction> {
        let action = self
            .bindings
            .get(&(InputContext::Global, key.clone()))
            .or_else(|| self.bindings.get(&(context, key.clone())))
            .copied()?;
        if context != InputContext::Gameplay
            && matches!(
                action,
                InputAction::MoveForward
                    | InputAction::MoveBackward
                    | InputAction::MoveLeft
                    | InputAction::MoveRight
                    | InputAction::MoveUp
                    | InputAction::MoveDown
                    | InputAction::Jump
                    | InputAction::Interact
                    | InputAction::Fire
            )
        {
            return None;
        }
        if matches!(context, InputContext::Console | InputContext::Modal)
            && action == InputAction::ToggleMap
        {
            return None;
        }
        if context == InputContext::Modal && action == InputAction::ToggleConsole {
            return None;
        }
        self.held.insert(key, action);
        Some(action)
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn clear_held(&mut self) {
        self.held.clear();
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn is_active(&self, action: InputAction) -> bool {
        self.held.values().any(|held| *held == action)
    }
}
