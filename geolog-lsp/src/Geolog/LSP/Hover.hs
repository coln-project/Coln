module Geolog.LSP.Hover where

import Geolog.LSP.Types (DLogLspM)
import Language.LSP.Protocol.Message
import Language.LSP.Protocol.Types
import Language.LSP.Server

hoverHandler :: Handlers DLogLspM
hoverHandler = requestHandler SMethod_TextDocumentHover $ \_ responder -> do
  responder
    ( Right
        . InL
        $ Hover
          { _contents = InL . mkMarkdown $ "unimplemented"
          , _range = Nothing
          }
    )
