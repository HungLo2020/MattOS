"""Shared helpers for independently maintained MattOS third-party packages."""

from .framework import (
    BuildResult,
    PackageRecipe,
    RecipeError,
    autotools_build_install,
    build_in_container,
    cmake_build_install,
    command,
    download,
    extract_archive,
    fetch_json,
    finalize_package,
    github_latest_release,
    github_source_archive,
    require_tools,
    package_staging,
    run_recipe,
    sha256_file,
    validate_package_artifact,
    validate_repository,
    write_control,
    write_provenance,
)

__all__ = [
    "BuildResult", "PackageRecipe", "RecipeError", "autotools_build_install",
    "build_in_container", "cmake_build_install",
    "command", "download", "extract_archive", "fetch_json", "finalize_package",
    "github_latest_release", "github_source_archive", "require_tools",
    "package_staging", "run_recipe", "sha256_file", "validate_package_artifact", "write_control",
    "validate_repository", "write_provenance",
]
