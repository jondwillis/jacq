---
name: datadog-dashboards
description: Create and edit Datadog dashboard JSON for import. Use when building dashboards, adding widgets, or configuring metrics queries.
---

# Datadog Dashboards

## Primary Reference: Use `search_datadog_docs`

For JSON structure, widget configuration, and field definitions, always use the `search_datadog_docs` MCP tool first. It provides up-to-date answers with examples.

## Style Guidance

- Always set units on y-axes and query values.
- Prefer percentile-based views (`p50`, `p95`, `p99`) for latency metrics.
- Keep widget titles concise and descriptive.

## Exporting and Importing Dashboard JSON

To export:
1. Open dashboard in Datadog
2. Open Configure
3. Copy dashboard JSON

To import:
1. Open Configure
2. Choose import dashboard JSON
3. Paste updated JSON
