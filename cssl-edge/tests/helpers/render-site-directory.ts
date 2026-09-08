import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { createElement, type ComponentType } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import ts from 'typescript';

// § actual component + actual dependencies → SSR; CSS names only shimmed.
// N! SSR coverage ⇒ browser layout|interaction acceptance.
export function renderSiteDirectory(): string {
  const componentPath = path.join(process.cwd(), 'components/site/SiteDirectory.tsx');
  const componentRequire = createRequire(componentPath);
  const componentModule = { exports: {} as { default: ComponentType } };
  const code = ts.transpileModule(fs.readFileSync(componentPath, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, esModuleInterop: true },
  }).outputText;
  new Function('require', 'module', 'exports', code)(
    (id: string) => id.endsWith('.module.css')
      ? { __esModule: true, default: new Proxy({}, { get: (_target, name) => name }) }
      : componentRequire(id),
    componentModule,
    componentModule.exports,
  );
  return renderToStaticMarkup(createElement(componentModule.exports.default));
}
