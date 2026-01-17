//! App detection and categorization
//!
//! Tracks active applications and provides category-based defaults for writing modes.
//! Actual app detection is done from Swift via NSWorkspace and passed to Rust via FFI.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use tracing::debug;

use crate::types::{AppCategory, AppContext, WritingMode};

/// Registry of known apps and their categories
pub struct AppRegistry {
    /// Mapping of app name patterns to categories
    categories: HashMap<String, AppCategory>,
    /// Bundle ID to category mapping
    bundle_categories: HashMap<String, AppCategory>,
}

impl Default for AppRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AppRegistry {
    /// Create a new registry with default app mappings
    pub fn new() -> Self {
        let mut categories = HashMap::new();
        let mut bundle_categories = HashMap::new();

        // Email clients
        for app in ["mail", "gmail", "outlook", "superhuman", "spark", "airmail"] {
            categories.insert(app.to_string(), AppCategory::Email);
        }
        bundle_categories.insert("com.apple.mail".to_string(), AppCategory::Email);
        bundle_categories.insert("com.microsoft.Outlook".to_string(), AppCategory::Email);
        bundle_categories.insert("com.superhuman.mail".to_string(), AppCategory::Email);

        // Slack/Teams/Discord
        for app in ["slack", "discord", "teams", "zoom"] {
            categories.insert(app.to_string(), AppCategory::Slack);
        }
        bundle_categories.insert("com.tinyspeck.slackmacgap".to_string(), AppCategory::Slack);
        bundle_categories.insert("com.hnc.Discord".to_string(), AppCategory::Slack);
        bundle_categories.insert("com.microsoft.teams".to_string(), AppCategory::Slack);

        // Code editors
        for app in [
            "code",
            "visual studio code",
            "cursor",
            "xcode",
            "intellij",
            "vim",
            "nvim",
            "neovim",
            "sublime",
            "atom",
            "windsurf",
            "zed",
        ] {
            categories.insert(app.to_string(), AppCategory::Code);
        }
        bundle_categories.insert("com.microsoft.VSCode".to_string(), AppCategory::Code);
        bundle_categories.insert("com.apple.dt.Xcode".to_string(), AppCategory::Code);
        bundle_categories.insert("com.todesktop.cursor".to_string(), AppCategory::Code);
        bundle_categories.insert("dev.zed.Zed".to_string(), AppCategory::Code);

        // Documents
        for app in [
            "pages",
            "word",
            "google docs",
            "notion",
            "obsidian",
            "bear",
            "notes",
            "craft",
        ] {
            categories.insert(app.to_string(), AppCategory::Documents);
        }
        bundle_categories.insert("com.apple.iWork.Pages".to_string(), AppCategory::Documents);
        bundle_categories.insert("com.microsoft.Word".to_string(), AppCategory::Documents);
        bundle_categories.insert("notion.id".to_string(), AppCategory::Documents);
        bundle_categories.insert("md.obsidian".to_string(), AppCategory::Documents);

        // Social media
        for app in [
            "twitter",
            "x",
            "facebook",
            "instagram",
            "linkedin",
            "tweetbot",
            "twitterrific",
        ] {
            categories.insert(app.to_string(), AppCategory::Social);
        }

        // Browsers
        for app in [
            "safari", "chrome", "firefox", "arc", "brave", "edge", "opera",
        ] {
            categories.insert(app.to_string(), AppCategory::Browser);
        }
        bundle_categories.insert("com.apple.Safari".to_string(), AppCategory::Browser);
        bundle_categories.insert("com.google.Chrome".to_string(), AppCategory::Browser);
        bundle_categories.insert("org.mozilla.firefox".to_string(), AppCategory::Browser);
        bundle_categories.insert(
            "company.thebrowser.Browser".to_string(),
            AppCategory::Browser,
        );

        // Terminals
        for app in [
            "terminal",
            "iterm",
            "iterm2",
            "warp",
            "kitty",
            "alacritty",
            "hyper",
            "ghostty",
        ] {
            categories.insert(app.to_string(), AppCategory::Terminal);
        }
        bundle_categories.insert("com.apple.Terminal".to_string(), AppCategory::Terminal);
        bundle_categories.insert("com.googlecode.iterm2".to_string(), AppCategory::Terminal);
        bundle_categories.insert("dev.warp.Warp-Stable".to_string(), AppCategory::Terminal);
        bundle_categories.insert("net.kovidgoyal.kitty".to_string(), AppCategory::Terminal);

        Self {
            categories,
            bundle_categories,
        }
    }

    /// Categorize an app by name and optional bundle ID
    pub fn categorize(&self, app_name: &str, bundle_id: Option<&str>) -> AppCategory {
        // first try bundle ID (most accurate)
        if let Some(bid) = bundle_id
            && let Some(&cat) = self.bundle_categories.get(bid)
        {
            return cat;
        }

        // then try app name
        let name_lower = app_name.to_lowercase();
        for (pattern, &category) in &self.categories {
            if name_lower.contains(pattern) {
                return category;
            }
        }

        // fall back to the types.rs implementation
        AppCategory::from_app(app_name, bundle_id)
    }

    /// Get suggested writing mode for a category
    pub fn suggested_mode(&self, category: AppCategory) -> WritingMode {
        WritingMode::suggested_for_category(category)
    }

    /// Add a custom app category mapping
    pub fn add_mapping(&mut self, app_name: &str, category: AppCategory) {
        self.categories.insert(app_name.to_lowercase(), category);
    }

    /// Add a custom bundle ID mapping
    pub fn add_bundle_mapping(&mut self, bundle_id: &str, category: AppCategory) {
        self.bundle_categories
            .insert(bundle_id.to_string(), category);
    }
}

/// Tracks the currently active app and app switch history
pub struct AppTracker {
    registry: AppRegistry,
    /// Currently active app
    current_app: RwLock<Option<AppContext>>,
    /// Recent app history (for analytics)
    history: RwLock<Vec<AppSwitch>>,
    /// Max history entries to keep
    max_history: usize,
}

/// Record of an app switch
#[derive(Debug, Clone)]
pub struct AppSwitch {
    pub app: AppContext,
    pub switched_at: DateTime<Utc>,
    pub duration_ms: Option<u64>,
}

impl Default for AppTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AppTracker {
    pub fn new() -> Self {
        Self {
            registry: AppRegistry::new(),
            current_app: RwLock::new(None),
            history: RwLock::new(Vec::new()),
            max_history: 100,
        }
    }

    pub fn with_registry(registry: AppRegistry) -> Self {
        Self {
            registry,
            current_app: RwLock::new(None),
            history: RwLock::new(Vec::new()),
            max_history: 100,
        }
    }

    /// Update the currently active app (called from Swift via FFI)
    pub fn set_active_app(
        &self,
        app_name: String,
        bundle_id: Option<String>,
        window_title: Option<String>,
    ) -> AppContext {
        let category = self.registry.categorize(&app_name, bundle_id.as_deref());

        let context = AppContext {
            app_name,
            bundle_id,
            window_title,
            category,
        };

        let now = Utc::now();

        // record the switch
        {
            let mut current = self.current_app.write();
            let mut hist = self.history.write();

            // update duration of previous app
            if current.is_some()
                && !hist.is_empty()
                && let Some(last) = hist.last_mut()
            {
                let duration = (now - last.switched_at).num_milliseconds().max(0) as u64;
                last.duration_ms = Some(duration);
            }

            // add new switch to history
            hist.push(AppSwitch {
                app: context.clone(),
                switched_at: now,
                duration_ms: None,
            });

            // trim history
            let len = hist.len();
            if len > self.max_history {
                hist.drain(0..len - self.max_history);
            }

            *current = Some(context.clone());
        }

        debug!("Active app: {} ({:?})", context.app_name, context.category);
        context
    }

    /// Get the currently active app
    pub fn current_app(&self) -> Option<AppContext> {
        self.current_app.read().clone()
    }

    /// Get the current app's category
    pub fn current_category(&self) -> AppCategory {
        self.current_app
            .read()
            .as_ref()
            .map(|a| a.category)
            .unwrap_or(AppCategory::Unknown)
    }

    /// Get the suggested writing mode for the current app
    pub fn suggested_mode(&self) -> WritingMode {
        let category = self.current_category();
        self.registry.suggested_mode(category)
    }

    /// Get recent app switch history
    pub fn recent_history(&self, limit: usize) -> Vec<AppSwitch> {
        let hist = self.history.read();
        hist.iter().rev().take(limit).cloned().collect()
    }

    /// Get app usage statistics
    pub fn usage_stats(&self) -> HashMap<String, u64> {
        let hist = self.history.read();
        let mut stats: HashMap<String, u64> = HashMap::new();

        for switch in hist.iter() {
            if let Some(duration) = switch.duration_ms {
                *stats.entry(switch.app.app_name.clone()).or_insert(0) += duration;
            }
        }

        stats
    }

    /// Access the registry for custom mappings
    pub fn registry_mut(&mut self) -> &mut AppRegistry {
        &mut self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_registry_categorization() {
        let registry = AppRegistry::new();

        assert_eq!(registry.categorize("Mail", None), AppCategory::Email);
        assert_eq!(
            registry.categorize("Visual Studio Code", None),
            AppCategory::Code
        );
        assert_eq!(registry.categorize("Slack", None), AppCategory::Slack);
        assert_eq!(
            registry.categorize("Unknown App", None),
            AppCategory::Unknown
        );
    }

    #[test]
    fn test_app_registry_bundle_id() {
        let registry = AppRegistry::new();

        assert_eq!(
            registry.categorize("Some App", Some("com.apple.mail")),
            AppCategory::Email
        );
        assert_eq!(
            registry.categorize("Some App", Some("com.tinyspeck.slackmacgap")),
            AppCategory::Slack
        );
    }

    #[test]
    fn test_app_tracker() {
        let tracker = AppTracker::new();

        // set first app
        let ctx = tracker.set_active_app("Slack".to_string(), None, Some("General".to_string()));
        assert_eq!(ctx.category, AppCategory::Slack);
        assert_eq!(tracker.current_category(), AppCategory::Slack);
        assert_eq!(tracker.suggested_mode(), WritingMode::Casual);

        // switch to email
        let ctx =
            tracker.set_active_app("Mail".to_string(), Some("com.apple.mail".to_string()), None);
        assert_eq!(ctx.category, AppCategory::Email);
        assert_eq!(tracker.suggested_mode(), WritingMode::Formal);

        // check history
        let history = tracker.recent_history(10);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_suggested_modes() {
        let registry = AppRegistry::new();

        assert_eq!(
            registry.suggested_mode(AppCategory::Email),
            WritingMode::Formal
        );
        assert_eq!(
            registry.suggested_mode(AppCategory::Social),
            WritingMode::VeryCasual
        );
        assert_eq!(
            registry.suggested_mode(AppCategory::Slack),
            WritingMode::Casual
        );
    }
}
