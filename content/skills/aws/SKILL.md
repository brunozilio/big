---
name: aws
description: Read before running the aws CLI or an AWS SDK against the cloud account the user connected. Covers checking who you are first, region, reading the real state of services, deploys and alarms, keeping output small, and what never to change.
---

# The AWS account

When a step has a cloud connection, the shell already holds short-lived
credentials for the AWS account and role the user chose (through
`AWS_PROFILE` or the `AWS_*` variables) and the region in `AWS_REGION`. Do not
ask for keys, do not write them to files and do not run `aws configure` or
`aws sso login`. Without a connection, `aws` fails; say that the step has no
cloud account instead of trying another way in.

## First

1. `aws sts get-caller-identity` — confirm the account and role before any
   other call, and name them in the report.
2. Keep the region from the environment. Pass `--region` only when the
   resource lives elsewhere, and say so.

## Reading the real state

- Prefer `describe`, `list` and `get` calls with `--query` and
  `--output json` or `table` to keep output small; add `--max-items` to lists.
- Deployed version: ECS `describe-services` (task definition, running and
  desired counts, events), Lambda `get-function` (version, last modified),
  CloudFormation `describe-stack-events` for a failed stack.
- Health: CloudWatch `describe-alarms --state-value ALARM`, and
  `aws logs filter-log-events` or `aws logs tail --since 30m` on the service's
  log group.
- Credentials expire within the hour; when a call says the token expired,
  report it instead of retrying in a loop.

## Never

- Change anything in the account (create, update, delete, put, start, stop,
  invoke with side effects) unless the user asked for that exact change in this
  conversation.
- Print secrets: skip `secretsmanager get-secret-value`, `ssm get-parameter
  --with-decryption` and environment blocks that hold credentials, or redact
  them.
