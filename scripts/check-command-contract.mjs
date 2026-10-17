import fs from 'node:fs'
import path from 'node:path'
import ts from 'typescript'

const ROOT = path.resolve(import.meta.dirname, '..')
const GENERATED = path.join(ROOT, 'src/services/tauri-commands.generated.ts')
const FRONTEND = path.join(ROOT, 'src/services/cmds.ts')
const program = ts.createProgram([GENERATED, FRONTEND], {
  strict: true,
  target: ts.ScriptTarget.Latest,
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler,
  skipLibCheck: true,
})
const checker = program.getTypeChecker()
const generatedFile = program.getSourceFile(GENERATED)
const frontendFile = program.getSourceFile(FRONTEND)
const failures = []

function propertyName(node) {
  if (ts.isIdentifier(node) || ts.isStringLiteral(node)) return node.text
  return undefined
}

function collectGeneratedCommands() {
  const commands = new Map()
  function visit(node) {
    if (
      ts.isVariableDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      node.name.text === 'commands' &&
      node.initializer &&
      ts.isObjectLiteralExpression(node.initializer)
    ) {
      for (const member of node.initializer.properties) {
        if (
          ts.isPropertyAssignment(member) &&
          propertyName(member.name) &&
          (ts.isArrowFunction(member.initializer) || ts.isFunctionExpression(member.initializer))
        ) {
          const signature = checker.getSignatureFromDeclaration(member.initializer)
          if (signature) commands.set(propertyName(member.name), signature)
        }
      }
    }
    ts.forEachChild(node, visit)
  }
  visit(generatedFile)
  return commands
}

const generatedCommands = collectGeneratedCommands()
const frontendCommands = new Set()

function expectedCommandName(command) {
  return command.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase())
}

function typeDescription(type, location) {
  return checker.typeToString(type, location, ts.TypeFormatFlags.NoTruncation)
}

function equivalentTypes(left, right) {
  return checker.isTypeAssignableTo(left, right) && checker.isTypeAssignableTo(right, left)
}

function collectArguments(argument) {
  if (!argument || argument.kind === ts.SyntaxKind.NullKeyword) return new Map()
  if (!ts.isObjectLiteralExpression(argument)) return undefined

  const entries = new Map()
  for (const property of argument.properties) {
    if (ts.isShorthandPropertyAssignment(property)) {
      entries.set(property.name.text, property.name)
    } else if (ts.isPropertyAssignment(property)) {
      const name = propertyName(property.name)
      if (name) entries.set(name, property.initializer)
    } else {
      return undefined
    }
  }
  return entries
}

function visitFrontend(node) {
  if (
    ts.isCallExpression(node) &&
    ts.isIdentifier(node.expression) &&
    node.expression.text === 'invoke' &&
    node.arguments.length > 0 &&
    ts.isStringLiteral(node.arguments[0])
  ) {
    const command = node.arguments[0].text
    frontendCommands.add(command)
    const generatedName = expectedCommandName(command)
    const signature = generatedCommands.get(generatedName)
    const location = `cmds.ts:${frontendFile.getLineAndCharacterOfPosition(node.getStart()).line + 1}`

    if (!signature) {
      failures.push(`${location}: ${command} is missing from generated command contract`)
    } else {
      const expectedParameters = signature.getParameters()
      const actualArguments = collectArguments(node.arguments[1])

      if (actualArguments === undefined) {
        failures.push(`${location}: ${command} must pass a literal argument object so its keys can be checked`)
      } else {
        const expectedNames = expectedParameters.map((parameter) => parameter.name).sort()
        const actualNames = [...actualArguments.keys()].sort()
        if (JSON.stringify(expectedNames) !== JSON.stringify(actualNames)) {
          failures.push(
            `${location}: ${command} argument keys differ; frontend [${actualNames.join(', ')}], backend [${expectedNames.join(', ')}]`,
          )
        }

        for (const parameter of expectedParameters) {
          const valueNode = actualArguments.get(parameter.name)
          if (!valueNode) continue
          const actualType = checker.getTypeAtLocation(valueNode)
          const expectedType = checker.getTypeOfSymbolAtLocation(parameter, signature.declaration)
          if (!checker.isTypeAssignableTo(actualType, expectedType)) {
            failures.push(
              `${location}: ${command}.${parameter.name} frontend type ${typeDescription(actualType, valueNode)} is not assignable to backend type ${typeDescription(expectedType, signature.declaration)}`,
            )
          }
        }
      }

      if (!node.typeArguments?.length) {
        failures.push(`${location}: ${command} must declare its invoke result type`)
      } else {
        const frontendResult = checker.getTypeFromTypeNode(node.typeArguments[0])
        const backendPromise = signature.getReturnType()
        const backendResult = checker.getPromisedTypeOfPromise(backendPromise)
        if (backendResult && !equivalentTypes(frontendResult, backendResult)) {
          failures.push(
            `${location}: ${command} result type differs; frontend ${typeDescription(frontendResult, node)}, backend ${typeDescription(backendResult, signature.declaration)}`,
          )
        }
      }
    }
  }
  ts.forEachChild(node, visitFrontend)
}

visitFrontend(frontendFile)

if (failures.length > 0) {
  throw new Error(`Tauri command contract mismatch:\n${failures.join('\n')}`)
}

console.log(
  `[check:command-contract] verified frontend command names, argument keys/types, and result types (${generatedCommands.size} generated; ${frontendCommands.size} invoked)`,
)
