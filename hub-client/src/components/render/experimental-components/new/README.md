These can be used in hub client by creating .tsx files for them and putting
them in your frontmatter like so:

```
---
format: q2-debug
render-components:
  - "simple\\_strings.tsx"
  - html.tsx
  - comments.tsx
  - "html\\_slide.tsx"
  - "drag\\_div.tsx"
source-location: full
---
```