//! GUI-focused extensions to the MovieBox adapter.
//!
//! Fork-only module: preserves the provider's homepage *sections* (row
//! titles like "Trending", "New Releases", ...) so the OTT front-end can
//! render true Netflix-style rows instead of one flattened list.

use crate::models::BrowseMetrics;
use crate::providers::moviebox::adapt::moviebox_subject_json_to_catalog_item;
use crate::service::extract_browse_metrics;
use crate::providers::models::CatalogItem;

/// One homepage section (a carousel/row on the provider site).
pub struct HomeSection {
    pub title: String,
    pub items: Vec<CatalogItem>,
}

/// Parse the homepage payload into ordered sections, de-duplicating items
/// across sections by subject id.
pub fn moviebox_homepage_json_to_sections(
    payload: &serde_json::Value,
) -> (Vec<HomeSection>, std::collections::HashMap<String, BrowseMetrics>) {
    let mut sections: Vec<HomeSection> = Vec::new();
    let mut metrics_map = std::collections::HashMap::new();
    let mut seen = std::collections::HashSet::new();

    let groups = payload
        .get("items")
        .and_then(|i| i.as_array())
        .or_else(|| payload.as_array());

    let Some(groups_arr) = groups else {
        return (sections, metrics_map);
    };

    for group in groups_arr {
        // Section title candidates (varies by endpoint version).
        let title = group
            .get("title")
            .or_else(|| group.get("name"))
            .or_else(|| group.get("sectionTitle"))
            .and_then(|t| t.as_str())
            .unwrap_or("More to watch")
            .trim()
            .to_string();

        let mut subjects: Vec<&serde_json::Value> = Vec::new();
        if let Some(banner) = group
            .get("banner")
            .and_then(|b| b.get("banners"))
            .and_then(|b| b.as_array())
        {
            for b_item in banner {
                if let Some(subject) = b_item.get("subject") {
                    subjects.push(subject);
                }
            }
        }
        if let Some(custom_data) = group
            .get("customData")
            .and_then(|c| c.get("items"))
            .and_then(|i| i.as_array())
        {
            for c_item in custom_data {
                if let Some(subject) = c_item.get("subject") {
                    subjects.push(subject);
                }
            }
        }
        if let Some(list) = group.get("subjects").and_then(|s| s.as_array()) {
            subjects.extend(list.iter());
        }

        let mut items = Vec::new();
        for (index, subject_val) in subjects.into_iter().enumerate() {
            if let Some(item) = moviebox_subject_json_to_catalog_item(subject_val) {
                if seen.insert(item.id.value.clone()) {
                    let mut metric = extract_browse_metrics(subject_val);
                    if metric.trending.is_none() {
                        metric.trending = Some((1000 - index.min(999)) as f64);
                    }
                    metrics_map.insert(item.id.value.clone(), metric);
                    items.push(item);
                }
            }
        }

        if !items.is_empty() {
            sections.push(HomeSection { title, items });
        }
    }

    (sections, metrics_map)
}
