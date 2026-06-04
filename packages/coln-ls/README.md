# `coln-ls`
A Language Server Protocol implementation for the coln language.

## Installing the vscode Extension
Currently the extension is not published to the vscode marketplace. In order to install the extension you will need to manually install the `.vsix` by selecting "Install from VSIX" on the extensions pane, then selecting the extension package. This can be retrieved as an artefact from any passing `vscode` pipeline. Alternatively if you wish to build the extension from source you can run the following from the project root:
```sh
./shake vsce
```
And select the package produced in `coln-ls/client`

## Capabilities
Currently the server supports the following:
- *Syntax Highlighting* : Using `coln-compiler`'s parser
- *Diagnostics* : Using `diagnostician`

And the following are yet to be implemented:
- *Hover* :
    > Currently we have a mapping of `AST Element -> Location`, however for hover documentation we will need the opposite. 
    Given a location, we will need to retrieve the AST element at that location. We can then display whatever information we desire.
- *Go To Definition* : 
    > Similarly for goto-definition we will need an awareness of what AST element is under the user's cursor, so that we can then decide where this element was defined. Also decisions will probably have to be made about what to point to for a definition.
- *Symbol Search* : 
    > This shouldn't be that difficult. We already have a list of all the elements in the syntax tree. The question is really what we want to display to the user as a symbol. Is it everything classed as `AIdent`? When looking up a symbol we should point to it's definition (whatever that means, decided above).
- *Actions* :
    > These can be added when they are deemed useful. Possible actions include:
    - Renaming `AIdent`'s

