# ═══════════════════════════════════════════════════════════════════════════
# Kora Docker Buildx Bake Configuration
# ═══════════════════════════════════════════════════════════════════════════

variable "REGISTRY" {
  default = "ghcr.io/refcell/kora"
}

variable "PLATFORMS" {
  default = "linux/amd64,linux/arm64"
}

variable "GIT_REF_NAME" {
  default = "main"
}

variable "BUILD_PROFILE" {
  default = "release"
}

# ───────────────────────────────────────────────────────────────────────────
# Groups
# ───────────────────────────────────────────────────────────────────────────

group "default" {
  targets = ["kora"]
}

group "all" {
  targets = ["kora", "kora-dev"]
}

# ───────────────────────────────────────────────────────────────────────────
# Base target for shared configuration
# ───────────────────────────────────────────────────────────────────────────

target "docker-metadata-action" {
  tags = ["${REGISTRY}/kora:${GIT_REF_NAME}"]
}

# ───────────────────────────────────────────────────────────────────────────
# Production build - multi-platform
# ───────────────────────────────────────────────────────────────────────────

target "kora" {
  inherits   = ["docker-metadata-action"]
  context    = ".."
  dockerfile = "docker/Dockerfile"
  platforms  = split(",", PLATFORMS)
  args = {
    BUILD_PROFILE = BUILD_PROFILE
  }
}

# ───────────────────────────────────────────────────────────────────────────
# Local development build - single platform, local only
# ───────────────────────────────────────────────────────────────────────────

target "kora-local" {
  context    = ".."
  dockerfile = "docker/Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = ["kora:local"]
  args = {
    BUILD_PROFILE = "release"
  }
}

# ───────────────────────────────────────────────────────────────────────────
# Development build with debug symbols
# ───────────────────────────────────────────────────────────────────────────

target "kora-dev" {
  context    = ".."
  dockerfile = "docker/Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = ["kora:dev"]
  args = {
    BUILD_PROFILE = "dev"
  }
}
