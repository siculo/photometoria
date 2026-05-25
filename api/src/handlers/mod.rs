// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

//! HTTP request handlers

pub mod app_error;
pub mod catalogs;
pub mod health;
pub mod jobs;
pub mod photos;
pub mod project;
pub mod providers;
pub mod upload_photos;

pub mod info;
#[cfg(test)]
pub mod test_utils;
