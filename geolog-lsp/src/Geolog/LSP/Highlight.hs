module Geolog.LSP.Highlight where

import Control.Monad.Trans
import Data.Either (fromRight)
import Data.Map qualified as M
import Data.Maybe (maybeToList)
import Geolog.LSP.Types (DLogLspM, FileParseState (FileParseState), LSPState (..))
import Geolog.LSP.Utils (currentBufferUri)
import Language.LSP.Protocol.Message
import Language.LSP.Protocol.Types
import Language.LSP.Server
import FNotation
import Diagnostician
import Data.IORef (readIORef)

tokenHandler :: Handlers DLogLspM
tokenHandler = requestHandler SMethod_TextDocumentSemanticTokensFull $ \req responder -> do
  LSPState parseRef <- getConfig
  p <- lift . readIORef $ parseRef
  let uri = currentBufferUri req
  responder $
    case M.lookup uri p of
      Nothing ->
        Left $ TResponseError (InL LSPErrorCodes_RequestFailed) "Doc not found in bundle" Nothing
      Just (FileParseState txt ntn) ->
        Right
          . InL
          . fromRight (SemanticTokens Nothing [])
          . makeSemanticTokens defaultSemanticTokensLegend
          $ ntn >>= highlightNtn (newFile (show uri) txt)

highlightNtn :: File -> Ntn -> [SemanticTokenAbsolute]
highlightNtn f = \case
  App n1 n2 -> concatMap h $ n1 : n2
  Infix n1 n2 n3 -> concatMap h [n1, n2, n3]
  Block _ n ns _ -> concatMap h $ maybeToList n ++ ns
  Decl name n (Span start _) -> t (Span start (start + (length . show $ name))) SemanticTokenTypes_Keyword ++ h n
  Ident _ s -> t s SemanticTokenTypes_Type
  Keyword _ s -> t s SemanticTokenTypes_Operator
  Field _ s -> t s SemanticTokenTypes_Method
  Int _ s -> t s SemanticTokenTypes_Number
  String _ s -> t s SemanticTokenTypes_String
  Tuple ns _ -> concatMap h ns
  Tag _ _ -> []
  Error _ -> []
  where
    h = highlightNtn f
    t s tok = [tokenFromSpan f s tok]

tokenFromSpan :: File -> Span -> SemanticTokenTypes -> SemanticTokenAbsolute
tokenFromSpan f (Span {start, end}) tokenType =
  let (startLine, startCol) = srcOf f start
   in SemanticTokenAbsolute
        { _line = fromIntegral startLine,
          _startChar = fromIntegral startCol,
          _length = fromIntegral $ end - start,
          _tokenType = tokenType,
          _tokenModifiers = []
        }
