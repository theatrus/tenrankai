#!/usr/bin/env node

/**
 * CSS Custom Property Validator
 *
 * Validates that all CSS custom properties (variables) used in var()
 * are defined somewhere in the CSS files.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const staticDir = path.join(__dirname, '..', 'static');

// Collect all CSS files
function findCssFiles(dir) {
  const files = [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory() && entry.name !== 'dist') {
      files.push(...findCssFiles(fullPath));
    } else if (entry.isFile() && entry.name.endsWith('.css')) {
      files.push(fullPath);
    }
  }

  return files;
}

// Extract defined custom properties from CSS content
function extractDefinitions(content) {
  const definitions = new Set();
  // Match --property-name: (definition)
  const regex = /(--[\w-]+)\s*:/g;
  let match;

  while ((match = regex.exec(content)) !== null) {
    definitions.add(match[1]);
  }

  return definitions;
}

// Extract used custom properties from CSS content
function extractUsages(content, filePath) {
  const usages = [];
  // Match var(--property-name) with optional fallback
  const regex = /var\(\s*(--[\w-]+)/g;
  let match;

  // Track line numbers
  const lines = content.split('\n');
  let currentPos = 0;

  while ((match = regex.exec(content)) !== null) {
    // Calculate line number
    let lineNumber = 1;
    let charCount = 0;
    for (let i = 0; i < lines.length; i++) {
      charCount += lines[i].length + 1; // +1 for newline
      if (charCount > match.index) {
        lineNumber = i + 1;
        break;
      }
    }

    usages.push({
      variable: match[1],
      file: filePath,
      line: lineNumber
    });
  }

  return usages;
}

// Main validation function
function validateCssVariables() {
  const cssFiles = findCssFiles(staticDir);

  if (cssFiles.length === 0) {
    console.log('No CSS files found in', staticDir);
    return true;
  }

  console.log(`Validating CSS variables in ${cssFiles.length} files...\n`);

  // Collect all definitions and usages
  const allDefinitions = new Set();
  const allUsages = [];

  for (const file of cssFiles) {
    const content = fs.readFileSync(file, 'utf-8');
    const relativePath = path.relative(path.join(__dirname, '..'), file);

    const definitions = extractDefinitions(content);
    definitions.forEach(d => allDefinitions.add(d));

    const usages = extractUsages(content, relativePath);
    allUsages.push(...usages);
  }

  // Find undefined variables
  const undefinedUsages = allUsages.filter(u => !allDefinitions.has(u.variable));

  // Group by variable for cleaner output
  const undefinedByVar = new Map();
  for (const usage of undefinedUsages) {
    if (!undefinedByVar.has(usage.variable)) {
      undefinedByVar.set(usage.variable, []);
    }
    undefinedByVar.get(usage.variable).push(usage);
  }

  if (undefinedByVar.size === 0) {
    console.log(`✓ All ${allUsages.length} CSS variable usages are valid`);
    console.log(`  ${allDefinitions.size} custom properties defined`);
    return true;
  }

  // Report errors
  console.error(`✗ Found ${undefinedByVar.size} undefined CSS variables:\n`);

  for (const [variable, usages] of undefinedByVar) {
    console.error(`  ${variable}`);
    for (const usage of usages) {
      console.error(`    → ${usage.file}:${usage.line}`);
    }
    console.error('');
  }

  // Suggest similar defined variables
  console.error('Defined variables that might be intended:');
  for (const variable of undefinedByVar.keys()) {
    const similar = [...allDefinitions].filter(d => {
      // Simple similarity check - shared words
      const varWords = variable.replace('--', '').split('-');
      const defWords = d.replace('--', '').split('-');
      return varWords.some(w => defWords.includes(w));
    }).slice(0, 3);

    if (similar.length > 0) {
      console.error(`  ${variable} → maybe: ${similar.join(', ')}`);
    }
  }

  return false;
}

// Run validation
const success = validateCssVariables();
process.exit(success ? 0 : 1);
