<!-- Cover page — rendered as page 1 of the PDF -->

::: cover

# {{REPORT_TITLE}}

**Prepared for**: {{RECIPIENT}}
**Engagement**: {{ENGAGEMENT_ID}}
**Version**: {{VERSION}}
**Date**: {{DATE}}

---

{{#if CONFIDENTIALITY_RESTRICTED}}
**[RESTRICTED — DO NOT REDISTRIBUTE]**
{{/if}}
{{#if CONFIDENTIALITY_CONFIDENTIAL}}
**[CONFIDENTIAL]**
{{/if}}

---

Prepared by: {{AUTHOR}}
Anvil report skill v{{ANVIL_VERSION}} • Thread `{{THREAD_SLUG}}` • Iteration {{ITERATION}}

:::
