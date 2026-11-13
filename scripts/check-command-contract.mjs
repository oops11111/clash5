import fs from 'node:fs'
import path from 'node:path'
import ts from 'typescript'

const ROOT = path.resolve(import.meta.dirname, '..')
const GENERATED = path.join(ROOT, 'src/services/tauri-commands.generated.ts')
const FRONTEND = path.join(ROOT, 'src/services/cmds.ts')

// The 124 hand-written invoke() sites in src/services/cmds.ts are the legacy
// layer: new code must use the generated `commands` object, so the count may
// only shrink, never grow.
const MAX_HAND_WRITTEN_INVOKE_SITES = 124
const config = ts.getParsedCommandLineOfConfigFile(
  path.join(ROOT, 'tsconfig.json'),
  {},
  {
    ...ts.sys,
    onUnRecoverableConfigFileDiagnostic: (diagnostic) => {
      throw new Error(
        ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'),
      )
    },
  },
)
const program = ts.createProgram(config.fileNames, config.options)
const diagnostics = ts.getPreEmitDiagnostics(program)
if (diagnostics.length) {
  throw new Error(
    ts.formatDiagnosticsWithColorAndContext(diagnostics, {
      getCurrentDirectory: () => ROOT,
      getCanonicalFileName: (file) => file,
      getNewLine: () => '\n',
    }),
  )
}
const checker = program.getTypeChecker()
const generatedFile = program.getSourceFile(GENERATED)
const frontendFile = program.getSourceFile(FRONTEND)
const failures = []
const invokeImport = frontendFile.statements
  .filter(ts.isImportDeclaration)
  .find((node) => node.moduleSpecifier.text === '@tauri-apps/api/core')
  .importClause.namedBindings.elements.find(
    (node) => node.name.text === 'invoke',
  )
const invokeSymbol = checker.getAliasedSymbol(
  checker.getSymbolAtLocation(invokeImport.name),
)

function commandInvocation(node) {
  if (
    ts.isCallExpression(node) &&
    ts.isIdentifier(node.expression) &&
    node.expression.text === '__TAURI_INVOKE'
  ) {
    return node
  }
  return ts.forEachChild(node, commandInvocation)
}

function propertyName(node) {
  if (ts.isIdentifier(node) || ts.isStringLiteral(node)) return node.text
  return undefined
}

// Method name (camelCase) -> command name (snake_case), plus the `commands`
// variable declaration itself, so `commands.foo()` call sites can be
// recognised as calls of the generated command.
const generatedMethodNames = new Map()
let generatedCommandsDeclaration = null

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
      generatedCommandsDeclaration = node
      for (const member of node.initializer.properties) {
        if (
          ts.isPropertyAssignment(member) &&
          propertyName(member.name) &&
          (ts.isArrowFunction(member.initializer) ||
            ts.isFunctionExpression(member.initializer))
        ) {
          const signature = checker.getSignatureFromDeclaration(
            member.initializer,
          )
          const invocation = commandInvocation(member.initializer)
          if (
            signature &&
            invocation &&
            ts.isStringLiteral(invocation.arguments[0])
          ) {
            commands.set(invocation.arguments[0].text, signature)
            // Method names are camelCase (clash5PurgeOrphans) while the
            // command names are snake_case (clash5_purge_orphans).
            generatedMethodNames.set(
              propertyName(member.name),
              invocation.arguments[0].text,
            )
          } else {
            throw new Error(
              `Cannot inspect generated command ${propertyName(member.name)}`,
            )
          }
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
const generatedObjectCommands = new Set()
let frontendInvokeSites = 0

function checkRegisteredCommands() {
  const source = fs.readFileSync(
    path.join(ROOT, 'src-tauri/src/lib.rs'),
    'utf8',
  )
  const registries = [
    ...source.matchAll(/tauri::generate_handler!\s*\[([^\]]+)\]/g),
  ]
  if (!registries.length) throw new Error('No Tauri command registry found')
  const registered = new Set()
  for (const [, entries] of registries) {
    for (const entry of entries
      .replace(/\/\/[^\n]*/g, '')
      .split(',')
      .map((value) => value.trim())
      .filter(Boolean)) {
      if (!/^(?:\w+::)*\w+$/.test(entry)) {
        throw new Error(`Cannot inspect registered command: ${entry}`)
      }
      registered.add(entry.split('::').at(-1))
    }
  }
  for (const command of registered) {
    if (!generatedCommands.has(command))
      failures.push(`Registered command ${command} has no generated schema`)
  }
  for (const command of generatedCommands.keys()) {
    if (!registered.has(command))
      failures.push(`Generated command ${command} is not registered`)
  }
  return registered
}

/**
 * If the node is a `commands.foo()` call on the generated commands object,
 * return the underlying command name; otherwise undefined. This keeps the
 * idle-command output correct once new code migrates to the generated object.
 */
function generatedObjectCall(node) {
  if (!generatedCommandsDeclaration) return undefined
  if (!ts.isCallExpression(node)) return undefined
  const target = node.expression
  if (!ts.isPropertyAccessExpression(target)) return undefined
  if (!ts.isIdentifier(target.expression)) return undefined
  if (target.expression.text !== 'commands') return undefined
  if (!ts.isIdentifier(target.name)) return undefined
  let symbol = checker.getSymbolAtLocation(target.expression)
  if (!symbol) return undefined
  if (symbol.flags & ts.SymbolFlags.Alias)
    symbol = checker.getAliasedSymbol(symbol)
  if (!symbol?.declarations?.includes(generatedCommandsDeclaration))
    return undefined
  return generatedMethodNames.get(target.name.text)
}

function typeDescription(type, location) {
  return checker.typeToString(type, location, ts.TypeFormatFlags.NoTruncation)
}

function equivalentTypes(left, right) {
  return (
    checker.isTypeAssignableTo(left, right) &&
    checker.isTypeAssignableTo(right, left)
  )
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
  const generatedCommand = generatedObjectCall(node)
  if (generatedCommand) generatedObjectCommands.add(generatedCommand)
  let symbol = ts.isCallExpression(node)
    ? checker.getSymbolAtLocation(node.expression)
    : undefined
  if (symbol?.flags & ts.SymbolFlags.Alias)
    symbol = checker.getAliasedSymbol(symbol)
  if (symbol === invokeSymbol) {
    const source = node.getSourceFile()
    const location = `${path.relative(ROOT, source.fileName)}:${source.getLineAndCharacterOfPosition(node.getStart()).line + 1}`
    if (!node.arguments.length || !ts.isStringLiteral(node.arguments[0])) {
      failures.push(`${location}: invoke requires a literal command name`)
      return
    }
    const command = node.arguments[0].text
    if (command.startsWith('plugin:')) return
    frontendInvokeSites += 1
    frontendCommands.add(command)
    const signature = generatedCommands.get(command)

    if (!signature) {
      failures.push(
        `${location}: ${command} is missing from generated command contract`,
      )
    } else {
      const expectedParameters = signature.getParameters()
      const actualArguments = collectArguments(node.arguments[1])

      if (actualArguments === undefined) {
        failures.push(
          `${location}: ${command} must pass a literal argument object so its keys can be checked`,
        )
      } else {
        const expectedNames = expectedParameters
          .map((parameter) => parameter.name)
          .sort()
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
          const expectedType = checker.getTypeOfSymbolAtLocation(
            parameter,
            signature.declaration,
          )
          if (
            actualType.flags & ts.TypeFlags.Any ||
            !checker.isTypeAssignableTo(actualType, expectedType)
          ) {
            failures.push(
              `${location}: ${command}.${parameter.name} frontend type ${typeDescription(actualType, valueNode)} is not assignable to backend type ${typeDescription(expectedType, signature.declaration)}`,
            )
          }
        }
      }

      if (!node.typeArguments?.length) {
        failures.push(
          `${location}: ${command} must declare its invoke result type`,
        )
      } else {
        const frontendResult = checker.getTypeFromTypeNode(
          node.typeArguments[0],
        )
        const backendPromise = signature.getReturnType()
        const wireResult = checker.getPromisedTypeOfPromise(backendPromise)
        const okResult =
          wireResult?.isUnion() &&
          wireResult.types.find((variant) => {
            const status = variant.getProperty('status')
            return (
              status &&
              checker.getTypeOfSymbolAtLocation(status, signature.declaration)
                .value === 'ok'
            )
          })
        const backendResult = okResult
          ? checker.getTypeOfSymbolAtLocation(
              okResult.getProperty('data'),
              signature.declaration,
            )
          : wireResult
        if (
          !backendResult ||
          frontendResult.flags & ts.TypeFlags.Any ||
          !equivalentTypes(frontendResult, backendResult)
        ) {
          failures.push(
            `${location}: ${command} result type differs; frontend ${typeDescription(frontendResult, node)}, backend ${backendResult ? typeDescription(backendResult, signature.declaration) : 'missing Promise result'}`,
          )
        }
      }
    }
  }
  ts.forEachChild(node, visitFrontend)
}

const registeredCommands = checkRegisteredCommands()
for (const file of config.fileNames) {
  if (path.resolve(file) !== GENERATED)
    visitFrontend(program.getSourceFile(file))
}

if (frontendInvokeSites > MAX_HAND_WRITTEN_INVOKE_SITES) {
  failures.push(
    `Hand-written invoke sites grew to ${frontendInvokeSites} (max ${MAX_HAND_WRITTEN_INVOKE_SITES}); new code must use the generated commands object`,
  )
}

if (failures.length > 0) {
  throw new Error(`Tauri command contract mismatch:\n${failures.join('\n')}`)
}

const calledCommands = new Set([...frontendCommands, ...generatedObjectCommands])
const idleCommands = [...registeredCommands]
  .filter((command) => !calledCommands.has(command))
  .sort()
if (idleCommands.length > 0) {
  console.log(
    `[check:command-contract] registered-but-uncalled commands (${idleCommands.length}): ${idleCommands.join(', ')}`,
  )
}

console.log(
  `[check:command-contract] verified frontend command names, argument keys/types, and result types (${generatedCommands.size} generated; ${calledCommands.size} invoked; ${frontendInvokeSites} hand-written invoke sites)`,
)
