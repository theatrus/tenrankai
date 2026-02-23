terraform {
  required_version = ">= 1.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 6.0"
    }
  }
}

data "aws_region" "current" {}

resource "aws_dynamodb_table" "config" {
  name         = var.table_name
  billing_mode = var.billing_mode

  read_capacity  = var.billing_mode == "PROVISIONED" ? var.read_capacity : null
  write_capacity = var.billing_mode == "PROVISIONED" ? var.write_capacity : null

  hash_key  = "pk"
  range_key = "sk"

  attribute {
    name = "pk"
    type = "S"
  }

  attribute {
    name = "sk"
    type = "S"
  }

  point_in_time_recovery {
    enabled = var.enable_point_in_time_recovery
  }

  deletion_protection_enabled = var.enable_deletion_protection

  stream_enabled   = var.enable_streams
  stream_view_type = var.enable_streams ? var.stream_view_type : null

  tags = merge(var.tags, {
    Service = "tenrankai"
    Purpose = "config-storage"
  })
}

# IAM policy document granting the minimum permissions Tenrankai needs.
# Attach this to the ECS task role, EC2 instance profile, or Lambda role.
data "aws_iam_policy_document" "access" {
  statement {
    sid = "ConfigStorageReadWrite"
    actions = [
      "dynamodb:GetItem",
      "dynamodb:PutItem",
      "dynamodb:UpdateItem",
      "dynamodb:DeleteItem",
      "dynamodb:Query",
      "dynamodb:Scan",
      "dynamodb:BatchWriteItem",
    ]
    resources = [aws_dynamodb_table.config.arn]
  }
}

resource "aws_iam_policy" "access" {
  name        = "${var.table_name}-access"
  description = "Allows Tenrankai to read/write the ${var.table_name} config storage table"
  policy      = data.aws_iam_policy_document.access.json

  tags = var.tags
}
