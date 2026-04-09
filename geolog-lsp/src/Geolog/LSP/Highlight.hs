module Geolog.LSP.Highlight where

import Control.Monad.Trans
import Data.Either (fromRight)
import Data.IORef (readIORef)
import Data.Map qualified as M
import Data.Maybe (catMaybes)
import Data.Vector qualified as V
import Diagnostician
import FNotation.Tokens
import Geolog.LSP.Types (AnalyzedBuffer (AnalyzedBuffer), GLogLspM, LSPState (..))
import Geolog.LSP.Utils (currentBufferUri)
import Language.LSP.Protocol.Message
import Language.LSP.Protocol.Types
import Language.LSP.Server

tokenHandler :: Handlers GLogLspM
tokenHandler = requestHandler SMethod_TextDocumentSemanticTokensFull $ \req responder -> do
  LSPState parseRef <- getConfig
  p <- lift . readIORef $ parseRef
  let uri = currentBufferUri req
  responder $
    case M.lookup uri p of
      Nothing ->
        Left $ TResponseError (InL LSPErrorCodes_RequestFailed) "Doc not found in bundle" Nothing
      Just (AnalyzedBuffer _ Nothing _ _ _) ->
        Left $ TResponseError (InL LSPErrorCodes_RequestFailed) "Doc is not in a lexable state" Nothing
      Just (AnalyzedBuffer f (Just lexed) _ _ _) ->
        Right
          . InL
          . fromRight (SemanticTokens Nothing [])
          . makeSemanticTokens defaultSemanticTokensLegend
          . catMaybes
          . V.toList
          . fmap (highlightLexed f)
          $ lexed

highlightLexed :: File -> Token -> Maybe SemanticTokenAbsolute
highlightLexed f (Token tokType _ s) = do
  col <- tokColour tokType
  pure $ tokenFromSpan f s col

tokColour :: Kind -> Maybe SemanticTokenTypes
tokColour = \case
  Decl -> Just SemanticTokenTypes_Keyword
  End -> Just SemanticTokenTypes_Keyword
  AIdent -> Nothing
  AKeyword -> Just SemanticTokenTypes_Parameter
  SKeyword -> Just SemanticTokenTypes_Operator
  Block -> Just SemanticTokenTypes_Keyword
  Field -> Just SemanticTokenTypes_Method
  Int -> Just SemanticTokenTypes_Number
  String -> Just SemanticTokenTypes_String
  LParen -> Just bcol
  RParen -> Just bcol
  LBrack -> Just bcol
  RBrack -> Just bcol
  LCurly -> Just bcol
  RCurly -> Just bcol
  SIdent -> Nothing
  Tag -> Nothing
  Comma -> Nothing
  Semicolon -> Nothing
  Nl -> Nothing
  Eof -> Nothing
  Error -> Nothing
 where
  bcol = SemanticTokenTypes_Type

tokenFromSpan :: File -> Span -> SemanticTokenTypes -> SemanticTokenAbsolute
tokenFromSpan f (Span{start, end}) tokenType =
  let (startLine, startCol) = srcOf f start
   in SemanticTokenAbsolute
        { _line = fromIntegral startLine
        , _startChar = fromIntegral startCol
        , _length = fromIntegral $ end - start
        , _tokenType = tokenType
        , _tokenModifiers = []
        }
