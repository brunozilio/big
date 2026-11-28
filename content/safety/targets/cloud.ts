export const hard = {
  credentials: {
    paths: ["~/.aws", "~/.config/gcloud", "~/.azure", "~/.oci", "~/.config/doctl", "~/.terraform.d"],
    secrets: [
      "~/.aws/credentials",
      "~/.config/gcloud/credentials.db",
      "~/.config/gcloud/application_default_credentials.json",
      "~/.config/gcloud/legacy_credentials",
      "~/.azure/msal_token_cache.json",
      "~/.azure/accessTokens.json",
      "~/.oci/oci_api_key.pem",
      "~/.config/doctl/config.yaml",
      "~/.terraform.d/credentials.tfrc.json",
    ],
  },
  sandbox_escape: {
    hosts: ["metadata.google.internal", "100.100.100.200"],
  },
};
