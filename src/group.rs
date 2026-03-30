use std::collections::HashMap;
use uuid::Uuid;

/// Identifies a window for matching purposes
#[derive(Debug, Clone)]
pub struct WindowMatcher {
    /// The process executable name (e.g., "Code.exe")
    pub process_name: Option<String>,
    /// Substring match on window title
    pub title_contains: Option<String>,
    /// Exact window class name (Win32)
    pub class_name: Option<String>,
}

impl WindowMatcher {
    pub fn by_process(name: &str) -> Self {
        Self {
            process_name: Some(name.to_string()),
            title_contains: None,
            class_name: None,
        }
    }

    pub fn by_title(title: &str) -> Self {
        Self {
            process_name: None,
            title_contains: Some(title.to_string()),
            class_name: None,
        }
    }
}

/// A runtime window handle with metadata
#[derive(Debug, Clone)]
pub struct TrackedWindow {
    pub hwnd: isize,
    pub process_name: String,
    pub title: String,
    pub class_name: String,
}

/// A group of windows that should be bound together
#[derive(Debug, Clone)]
pub struct WindowGroup {
    pub id: String,
    pub name: String,
    pub window_matchers: Vec<WindowMatcher>,
    pub enabled: bool,
    /// Whether to sync minimize/restore
    pub sync_minimize: bool,
    /// Whether to sync move/resize (keeping relative positions)
    pub sync_move: bool,
}

impl WindowGroup {
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            window_matchers: Vec::new(),
            enabled: true,
            sync_minimize: true,
            sync_move: true,
        }
    }

    pub fn add_matcher(&mut self, matcher: WindowMatcher) {
        self.window_matchers.push(matcher);
    }

    /// Check if a window matches any matcher in this group
    pub fn matches_window(&self, window: &TrackedWindow) -> bool {
        if !self.enabled {
            return false;
        }
        self.window_matchers.iter().any(|m| {
            let proc_match = m.process_name.as_ref().map_or(true, |p| {
                window.process_name.to_lowercase() == p.to_lowercase()
            });
            let title_match = m.title_contains.as_ref().map_or(true, |t| {
                window.title.to_lowercase().contains(&t.to_lowercase())
            });
            let class_match = m.class_name.as_ref().map_or(true, |c| {
                window.class_name.to_lowercase() == c.to_lowercase()
            });
            proc_match && title_match && class_match
        })
    }
}

/// Manages all window groups and runtime state
pub struct GroupManager {
    /// Defined groups
    pub groups: Vec<WindowGroup>,
    /// Currently tracked live windows mapped to their group id
    /// Key: hwnd (as isize), Value: group_id
    pub active_bindings: HashMap<isize, String>,
    /// Suppression flag to avoid recursive activation loops
    pub suppressed: bool,
}

impl GroupManager {
    pub fn new() -> Self {
        Self {
            groups: Vec::new(),
            active_bindings: HashMap::new(),
            suppressed: false,
        }
    }

    pub fn add_group(&mut self, group: WindowGroup) {
        self.groups.push(group);
    }

    pub fn remove_group(&mut self, group_id: &str) {
        self.groups.retain(|g| g.id != group_id);
        self.active_bindings.retain(|_, gid| gid != group_id);
    }

    /// Register a live window handle to a group
    pub fn bind_window(&mut self, hwnd: isize, group_id: &str) {
        self.active_bindings.insert(hwnd, group_id.to_string());
    }

    /// Unregister a window handle
    pub fn unbind_window(&mut self, hwnd: isize) {
        self.active_bindings.remove(&hwnd);
    }

    /// Find which group a window belongs to
    pub fn find_group_for_hwnd(&self, hwnd: isize) -> Option<&str> {
        self.active_bindings.get(&hwnd).map(|s| s.as_str())
    }

    /// Get all hwnds in the same group as the given hwnd (excluding itself)
    pub fn get_sibling_hwnds(&self, hwnd: isize) -> Vec<isize> {
        if let Some(group_id) = self.active_bindings.get(&hwnd) {
            self.active_bindings
                .iter()
                .filter(|(&h, gid)| h != hwnd && gid.as_str() == group_id)
                .map(|(&h, _)| h)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Try to match a window to a group and auto-bind it
    pub fn try_auto_bind(&mut self, window: &TrackedWindow) -> Option<String> {
        for group in &self.groups {
            if group.matches_window(window) {
                let gid = group.id.clone();
                self.active_bindings.insert(window.hwnd, gid.clone());
                return Some(gid);
            }
        }
        None
    }

    /// Create a group from a set of picked window handles
    pub fn create_group_from_hwnds(&mut self, name: &str, hwnds: Vec<(isize, String, String)>) -> String {
        let mut group = WindowGroup::new(name);
        for (_hwnd, process_name, _title) in &hwnds {
            // Use process name as the primary matcher
            if !group.window_matchers.iter().any(|m| {
                m.process_name.as_deref() == Some(process_name.as_str())
            }) {
                group.add_matcher(WindowMatcher::by_process(process_name));
            }
        }
        let gid = group.id.clone();
        // Bind the actual windows
        for (hwnd, _, _) in &hwnds {
            self.active_bindings.insert(*hwnd, gid.clone());
        }
        self.groups.push(group);
        gid
    }

    /// Get a snapshot of all groups for serialization
    pub fn get_groups(&self) -> &[WindowGroup] {
        &self.groups
    }
}
