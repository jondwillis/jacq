---
name: wireframes
description: Guide PMs and designers in creating comprehensive wireframes and mockups that accurately represent all backend API responses and data states. Use when creating wireframes or mockups for new features.
---

# Wireframe Creation

This skill guides PMs and designers in creating comprehensive wireframes and mockups that accurately represent all backend API responses and data states.

**Wireframe Creation Approach:**

1. **Backend-First Analysis**
   - Use semantic search (`codebase_search`) to identify all API endpoints and their response structures
   - Search for API routes, controllers, and response schemas within the project
   - Analyze existing API documentation, OpenAPI/Swagger specs, or GraphQL schemas
   - Document all possible response states: success, error, loading, empty states

2. **Data State Coverage**
   - Create wireframes for every data state:
     - **Success states**: Normal data display with various data volumes (empty, single item, multiple items, paginated)
     - **Error states**: Network errors, validation errors, server errors, not found states
     - **Loading states**: Initial load, refresh, pagination loading, form submission
     - **Edge cases**: Empty lists, missing data, long text truncation, image loading failures

3. **API Response Mocking**
   - For each wireframe, document the exact backend response structure it represents
   - Include sample JSON payloads or response examples
   - Mock all response fields, including nested objects and arrays
   - Account for optional vs required fields and their impact on UI

4. **When Creating Wireframes:**
   - Start by mapping all backend endpoints that the feature will consume
   - Create wireframes that show how each response type is displayed
   - Include annotations indicating which API endpoint and response structure each screen represents
   - Ensure wireframes cover all user flows, including error recovery paths
   - Document any assumptions about data structure or backend behavior
