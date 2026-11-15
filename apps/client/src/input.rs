mod controls;
pub(crate) use controls::PhysicalControl;
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
    EquipShovel,
    EquipPickaxe,
    StowTool,
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
            "shovel" => Self::EquipShovel,
            "pickaxe" => Self::EquipPickaxe,
            "stow" => Self::StowTool,
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
    bindings: HashMap<(InputContext, PhysicalControl), InputAction>,
    #[cfg(feature = "desktop")]
    held: HashMap<PhysicalControl, InputAction>,
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
            ("gameplay", "shovel", "Digit1"),
            ("gameplay", "pickaxe", "Digit2"),
            ("gameplay", "stow", "Digit0"),
        ] {
            result.insert(
                InputContext::parse(ctx).unwrap(),
                InputAction::parse(action).unwrap(),
                PhysicalControl::parse(key).expect("default control is supported"),
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
            let control = PhysicalControl::parse(key)
                .ok_or_else(|| format!("unsupported physical input `{key}`"))?;
            result
                .bindings
                .retain(|(c, _), a| *c != context || *a != action);
            result.insert(context, action, control)?;
        }
        Ok(result)
    }
    fn insert(
        &mut self,
        context: InputContext,
        action: InputAction,
        key: PhysicalControl,
    ) -> Result<(), String> {
        if self
            .bindings
            .iter()
            .any(|((c, k), a)| *c == context && *k == key && *a != action)
        {
            return Err(format!(
                "input `{}` has conflicting bindings in {} context",
                key.name(),
                context.name()
            ));
        }
        self.bindings.insert((context, key), action);
        Ok(())
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn release(&mut self, key: PhysicalControl) {
        self.held.remove(&key);
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn press(
        &mut self,
        key: PhysicalControl,
        context: InputContext,
    ) -> Option<InputAction> {
        let action = self
            .bindings
            .get(&(InputContext::Global, key))
            .or_else(|| self.bindings.get(&(context, key)))
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
                    | InputAction::EquipShovel
                    | InputAction::EquipPickaxe
                    | InputAction::StowTool
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
