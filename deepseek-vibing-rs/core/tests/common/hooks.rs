use deepseek_vibing_config::CONFIG_TOML_FILE;
use deepseek_vibing_config::ConfigLayerStack;
use deepseek_vibing_config::TomlValue;
use deepseek_vibing_core::config::Config;
use deepseek_vibing_features::Feature;
use deepseek_vibing_hooks::HookListEntry;
use deepseek_vibing_utils_absolute_path::AbsolutePathBuf;

pub fn trust_discovered_hooks(config: &mut Config) {
    if let Err(err) = config.features.enable(Feature::DeepSeekVibingHooks) {
        panic!("test config should allow feature update: {err}");
    }

    let listed = deepseek_vibing_hooks::list_hooks(deepseek_vibing_hooks::HooksConfig {
        feature_enabled: true,
        config_layer_stack: Some(config.config_layer_stack.clone()),
        ..deepseek_vibing_hooks::HooksConfig::default()
    });
    assert!(
        !listed.hooks.is_empty(),
        "trusted hook fixture should discover at least one hook"
    );
    trust_hooks(config, listed.hooks);
}

pub fn trust_hooks(config: &mut Config, hooks: Vec<HookListEntry>) {
    config.config_layer_stack = trusted_config_layer_stack(
        &config.config_layer_stack,
        &config.deepseek_vibing_home,
        hooks,
    );
}

pub fn trusted_config_layer_stack(
    config_layer_stack: &ConfigLayerStack,
    deepseek_vibing_home: &AbsolutePathBuf,
    hooks: Vec<HookListEntry>,
) -> ConfigLayerStack {
    let mut user_config = config_layer_stack
        .get_active_user_layer()
        .map(|layer| layer.config.clone())
        .unwrap_or_else(|| TomlValue::Table(Default::default()));
    let Some(user_table) = user_config.as_table_mut() else {
        panic!("user config should be a table");
    };
    let Some(hooks_table) = user_table
        .entry("hooks")
        .or_insert_with(|| TomlValue::Table(Default::default()))
        .as_table_mut()
    else {
        panic!("hooks config should be a table");
    };
    let Some(state_table) = hooks_table
        .entry("state")
        .or_insert_with(|| TomlValue::Table(Default::default()))
        .as_table_mut()
    else {
        panic!("hook state config should be a table");
    };
    for hook in hooks {
        let mut hook_state = TomlValue::Table(Default::default());
        let Some(hook_state_table) = hook_state.as_table_mut() else {
            panic!("hook state should be a table");
        };
        hook_state_table.insert(
            "trusted_hash".to_string(),
            TomlValue::String(hook.current_hash),
        );
        state_table.insert(hook.key, hook_state);
    }

    config_layer_stack.with_user_config(&deepseek_vibing_home.join(CONFIG_TOML_FILE), user_config)
}
