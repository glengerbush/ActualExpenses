---
status: accepted
---

# Publish server images for manual Coolify updates

The existing deployment is a Coolify Docker Image resource, and the user wants to retain its "Pull latest and restart" update workflow. Build the fork's production server/web image in GitHub Actions from `actualBudget/`, publish it to GitHub Container Registry, and configure Coolify to pull that image. This supersedes the initially proposed source build inside Coolify: it adds registry publishing to release automation but preserves the user's operational workflow and keeps build work off the deployment server.

Publishing an image must not deploy it automatically. Preserve the existing port and persistent data mount during migration; verify their live values before switching the image reference.
