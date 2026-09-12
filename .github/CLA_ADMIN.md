# CLA administration

Use the hosted [CLA Assistant](https://cla-assistant.io) to collect one-time
acceptance of [Systemless CLA v1.0](../CLA.md) and check subsequent PRs.
The public project stays GPL-3.0-or-later; this setup does not change the CLA.

## Activation status

This PR prepares the repository instructions. It does not connect the hosted
service, import prior agreements, or enable a required status check. Until the
steps below are complete, maintainers must verify acceptance before merging.

Ben Letchford is responsible for reviewing prior agreements and configuring
the service. No contributor is assumed to have signed merely because they
previously contributed.

## One-time setup

1. Create a GitHub Gist owned by the project maintainer, with a single file
   named `CLA.md` containing the exact current repository agreement.
   From a clean checkout of the approved default branch, GitHub CLI can do this:

   ```sh
   gh gist create --public CLA.md --desc "Systemless Contributor Agreement v1.0"
   ```

   Run this once and retain the resulting URL. Check the published text against
   the repository file before connecting it. Do not add acceptance instructions
   or personal information to the agreement itself.
2. Sign in at [cla-assistant.io](https://cla-assistant.io) with the maintainer
   account and review the requested GitHub permissions. Link only
   `benletchford/systemless` to the Gist. No repository PAT or custom GitHub
   Actions workflow is required for the hosted setup.
3. Record the Gist URL and revision, the source repository commit, CLA version,
   and activation date in the private administration record. Keep the Gist text
   fixed for v1.0. The service tracks Gist revisions and may request fresh
   acceptance after edits, including cosmetic edits.
4. Reconcile existing agreements using the procedure below.
5. Exercise the acceptance flow on a real, unmerged contributor PR. Verify that
   an unsigned contributor produces a failing CLA status and their own explicit
   acceptance makes it pass. Verify repeat contributions and a PR with multiple
   contributors. Unmapped commit authors or co-authors require manual review;
   do not assume a green status establishes every contributor's rights.
6. In GitHub repository settings, add the actual status emitted by the service
   to the required checks for `master`, using a branch rule or ruleset. Select
   the expected source app if GitHub offers it. Preserve existing protections.
   Verify that the merge UI blocks an unsigned PR. A green bot comment alone
   is not merge enforcement.
7. Export the signature list from the dashboard and retain it privately with
   the exact agreement revision. Back up exports periodically and before
   changing providers or the agreement.

After activation, new contributors follow the bot's signing link once.
Returning contributors covered by that version pass automatically.

## Recognising earlier agreements

Review each original email, signed document, or GitHub acceptance before
marking someone as covered. Verify identity, the exact terms accepted, and
whether those terms cover future contributions and the needed commercial
licensing permissions.

Keep a private register with these fields:

| Field | Purpose |
| --- | --- |
| GitHub login and numeric account ID | Match the contributor, including later renames |
| Agreement version or exact text | Establish which permissions were granted |
| Original acceptance date | Preserve the actual acceptance history |
| Evidence location | Locate the original message or document |
| Scope | Record whether past and future contributions are covered |
| Reviewed by and review date | Identify who checked the evidence |
| Service reconciliation method and date | Distinguish imports or exemptions from online signatures |

Use the service's import facility for verified prior acceptances where its
available format preserves the relevant identity and agreement version.
Keep the original evidence even after import. Do not invent an online signing
date, map different terms to v1.0 without review, or upload private correspondence.

If import cannot represent an earlier agreement accurately, retain the evidence
privately and use a narrowly scoped, documented service exemption where supported.
An exemption is an administrative decision, not a signature. If neither route
is suitable, request one-time online acceptance; do not bypass all CLA checks.

Review bot exemptions individually. A bot account does not establish ownership
or permission for the material it submits. Do not exempt all collaborators,
all historical contributors, or wildcard usernames.

## Routine handling

- For an unsigned contributor, let the service request acceptance. Do not sign
  on their behalf.
- For missing prompts or stale statuses, check the service connection, linked
  Gist, and GitHub identity, then request a service recheck.
- If the service is unavailable, hold the merge or record explicit acceptance
  and use the repository's documented, authorised exception process. Never
  manufacture a passing status.
- For substantive CLA changes, version the agreement deliberately, retain the
  old text and acceptance evidence, update the linked Gist, and verify the new
  acceptance flow. Earlier agreement rights are not erased by changing providers.
- Coverage of third-party code, dependencies, and game assets still requires
  separate licence review; a CLA check is not a complete rights audit.

## References

- [Hosted CLA Assistant documentation](https://github.com/cla-assistant/cla-assistant#readme)
- [Systemless licensing](../LICENSING.md)
- The separate [CLA Assistant GitHub Action](https://github.com/contributor-assistant/github-action)
  is archived. This setup uses the hosted service instead.
