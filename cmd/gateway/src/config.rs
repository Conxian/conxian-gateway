@@
     fn optional_env(key: &str) -> Option<String> {
         env::var(key).ok().and_then(|value| {
             let value = value.trim().to_string();
             (!value.is_empty()).then_some(value)
         })
     }
+
+    /// Compatibility helper: accept API_TOKEN or legacy CONXIAN_GATEWAY_AUTH_TOKEN.
+    fn get_api_token_compat() -> String {
+        // Try the primary canonical name first, then the legacy installer name.
+        let raw = env::var("API_TOKEN").or_else(|_| env::var("CONXIAN_GATEWAY_AUTH_TOKEN"));
+        let val = raw.unwrap_or_else(|_| panic!("API_TOKEN or CONXIAN_GATEWAY_AUTH_TOKEN must be set"));
+        let val = val.trim().to_string();
+        if val.is_empty() || val == API_TOKEN_SENTINEL {
+            panic!("API_TOKEN must be a non-empty secret (not {})", API_TOKEN_SENTINEL);
+        }
+        val
+    }
@@
-            api_token,
+            api_token: Self::get_api_token_compat(),
@@
     #[test]
     fn from_env_trims_secret_whitespace() {
@@
     }
+
+    #[test]
+    fn api_token_compatibility_accepts_legacy_name() {
+        let _guard = ENV_MUTEX.lock().unwrap();
+        let _env_restore = FullEnvRestore::new();
+        set_test_envs();
+
+        // Ensure legacy installer env name is accepted.
+        env::remove_var("API_TOKEN");
+        env::set_var("CONXIAN_GATEWAY_AUTH_TOKEN", "legacy-token-123");
+
+        let config = Config::from_env();
+        assert_eq!(config.api_token, "legacy-token-123");
+    }
