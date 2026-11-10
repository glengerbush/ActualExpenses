-- External connectors are retired. Financial rows/import metadata and all
-- original source uploads/upgrade backup artifacts remain untouched.
DROP TABLE bank_transaction_origins;
DROP TABLE bank_jobs;
DROP TABLE bank_request_log;
DROP TABLE bank_links;
DROP TABLE bank_remote_accounts;
DROP TABLE bank_gocardless_credentials;
DROP TABLE bank_credentials;
DROP TABLE bank_connections;
DROP TABLE bank_gocardless_limits;
DELETE FROM field_versions WHERE entity IN ('bank_connections','bank_remote_accounts','bank_links','bank_transaction_origins');
DELETE FROM budget_changes WHERE entity IN ('bank_connections','bank_remote_accounts','bank_links','bank_transaction_origins');
UPDATE budget_preferences
SET settings=json_remove(settings,'$.experimental.enableBanking','$.experimental.akahuBankSync'),revision=revision+1
WHERE json_type(settings,'$.experimental.enableBanking') IS NOT NULL
   OR json_type(settings,'$.experimental.akahuBankSync') IS NOT NULL;
