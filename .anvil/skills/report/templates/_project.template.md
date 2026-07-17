---
recipient: "Acme Corporation, Q2 Engagement"
engagement_id: "ACME-2026-Q2"
delivery_format: "pdf"             # pdf | latex | markdown
confidentiality_class: "internal"  # public | internal | confidential | restricted
customer: "acme"                   # OPTIONAL — cross-project customer-context slug.
                                   # Resolves <customers_dir>/<slug>/context.yaml
                                   # (default <repo_root>/customers/; override via
                                   # .anvil/config.json key report.customers_dir).
                                   # Omit the key entirely to leave the customer-
                                   # context tier off (byte-identical behavior).
# audience_class: "commercial"     # OPTIONAL — audience-class house-style switch
                                   # (closed vocabulary: commercial | defense |
                                   # internal; issue #450). Overrides the customer's
                                   # context.yaml `audience_class:` default; also the
                                   # sole locus for internal reports with NO customer.
                                   # Omit everywhere → byte-identical pre-#450
                                   # behavior. `defense` requires consumer-supplied
                                   # assets/audience/defense.md boilerplate (anvil
                                   # ships no legal text) and adds a DRAFT watermark.
prior_reports:
  - thread: findings
    final_version: 3
    delivered_at: "2026-04-12"
  - thread: interim
    final_version: 2
    delivered_at: "2026-05-01"
voice_notes: "Technical but accessible; recipient CTO is an engineer. Avoid sales tone."
---

# Engagement: Acme Corporation, Q2 2026

## Recipient context

(Who is the recipient organization? Who is the primary reader at that organization?
What is their technical background and what do they care most about hearing from us?
What is the history of the relationship? Any sensitivities to keep in mind?)

## Engagement scope

(What did the engagement contract say we would deliver? What is in scope and what is
explicitly out of scope? Are there hard constraints on what we can or cannot say?)

## Communication norms

(Preferred channels, escalation paths, recipient holidays/embargoes, formatting
preferences (anonymized vs. named individuals in body, page-count expectations, etc.))

## Notes on prior reports

(For each prior report listed in `prior_reports[]` above: what was said, what changed
since, anything that future reports should remain consistent with or explicitly
update.)
