module Geolog.LSP.Hover where

import Geolog.LSP.Types (GLogLspM)
import Language.LSP.Protocol.Message
import Language.LSP.Protocol.Types
import Language.LSP.Server

hoverHandler :: Handlers GLogLspM
hoverHandler = requestHandler SMethod_TextDocumentHover $ \_ responder -> do
  responder
    ( Right
        . InL
        $ Hover
          { _contents = InL . mkMarkdown $ "unimplemented",
            _range = Nothing
          }
    )
