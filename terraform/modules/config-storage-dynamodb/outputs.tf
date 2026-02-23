output "table_name" {
  description = "Name of the DynamoDB table"
  value       = aws_dynamodb_table.config.name
}

output "table_arn" {
  description = "ARN of the DynamoDB table"
  value       = aws_dynamodb_table.config.arn
}

output "table_stream_arn" {
  description = "ARN of the DynamoDB Stream (empty string if streams are disabled)"
  value       = var.enable_streams ? aws_dynamodb_table.config.stream_arn : ""
}

output "access_policy_arn" {
  description = "ARN of the IAM policy granting Tenrankai access to the table"
  value       = aws_iam_policy.access.arn
}

output "config_storage_url" {
  description = "The dynamodb:// URL to use in Tenrankai's config.toml"
  value       = "dynamodb://${aws_dynamodb_table.config.name}?region=${data.aws_region.current.id}"
}
