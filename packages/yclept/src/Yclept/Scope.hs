-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE GeneralizedNewtypeDeriving #-}
{-# LANGUAGE OverloadedRecordDot #-}

{- | The lexical-scope engine, a port of the OCaml @Scope@ / @ScopeSigs@.

A scope has two namespaces: a /visible/ one (what is in scope) and an
/export/ one (what will be exported).  The OCaml module realises this with
three /standard/ algebraic effects that are never overridden — mutable state
(the two tries), a reader (the export prefix), and a mutex (a re-entrancy
guard).  Here those become a monad transformer stack:

> ScopeT d t h c m = ReaderT (Env m d t h c) (StateT (ScopeState d t) m)

Every use of 'lift' is confined to the small set of primitive wrappers below
('getScope', 'putScope', 'askEnv', 'localEnv', 'liftBase'); the rest of the
module—and any client code—goes through those, so the transformer stack can
be changed in one place.

The /overridable/ effects (@not_found@, @shadow@, @hook@) are handled the
"Yclept.Modifier" way: a 'Handlers' bundle, carried in the reader
environment so that 'tryWith' can override it for a sub-computation exactly as
the OCaml @try_with@ did.
-}
module Yclept.Scope (
  -- * The scope monad
  ScopeT,
  Handlers (..),
  silence,
  Locked (..),

  -- * Runners
  run,
  runWith,
  tryWith,

  -- * Escaping to the base monad
  liftBase,

  -- * Name resolution
  resolve,

  -- * Inclusion (affects visible and export)
  includeSingleton,
  includeSubtree,

  -- * Importing (affects visible only)
  importSingleton,
  importSubtree,

  -- * Modifying namespaces
  modifyVisible,
  modifyExport,

  -- * Exporting
  exportVisible,
  getVisible,
  getExport,

  -- * Sections
  section,
) where

import Control.Exception (Exception, throw)
import Control.Monad.IO.Class (MonadIO (..))
import Control.Monad.Trans.Class (lift)
import Control.Monad.Trans.Reader (ReaderT, ask, local, runReaderT)
import Control.Monad.Trans.State.Strict (StateT, evalStateT, get, put)

import Yclept.Bwd (Bwd (Emp), (<@))
import Yclept.Language (Language)
import Yclept.Modifier (Handlers (..), silence)
import Yclept.Modifier qualified as Modifier
import Yclept.Trie (BwdPath, Path, Trie)
import Yclept.Trie qualified as Trie

-- | The reader environment: the export prefix and the current handler bundle.
data Env m d t h c = Env
  { exportPrefix :: BwdPath
  , handlers :: Handlers m d t h c
  }

{- | The mutable scope state.  @locked@ is the re-entrancy guard that stands in
for the OCaml @Algaeff.Mutex@.
-}
data ScopeState d t = ScopeState
  { visible :: Trie d t
  , export :: Trie d t
  , locked :: Bool
  }

{- | The scope monad transformer.  @d@\/@t@ are the data\/tag, @h@ the hook
label, @c@ the context, and @m@ the base monad (where handlers run).
-}
newtype ScopeT d t h c m a = ScopeT
  {unScopeT :: ReaderT (Env m d t h c) (StateT (ScopeState d t) m) a}
  deriving (Functor, Applicative, Monad)

instance (MonadIO m) => MonadIO (ScopeT d t h c m) where
  liftIO = liftBase . liftIO

{- | Raised when a scope operation is attempted while another operation on the
same scope is still in progress (the OCaml @Locked@).  This signals a serious
programming error.
-}
data Locked = Locked
  deriving (Show)

instance Exception Locked

-- ---------------------------------------------------------------------------
-- Primitive wrappers — the ONLY place 'lift' appears.
-- ---------------------------------------------------------------------------

{- | Run a base-monad action inside 'ScopeT'.  This is also how handler-driven
code (e.g. printing in an effect handler) reaches the base monad from within
a scope.
-}
liftBase :: (Monad m) => m a -> ScopeT d t h c m a
liftBase = ScopeT . lift . lift

getScope :: (Monad m) => ScopeT d t h c m (ScopeState d t)
getScope = ScopeT (lift get)

putScope :: (Monad m) => ScopeState d t -> ScopeT d t h c m ()
putScope = ScopeT . lift . put

askEnv :: (Monad m) => ScopeT d t h c m (Env m d t h c)
askEnv = ScopeT ask

localEnv :: (Monad m) => (Env m d t h c -> Env m d t h c) -> ScopeT d t h c m a -> ScopeT d t h c m a
localEnv f (ScopeT a) = ScopeT (local f a)

-- ---------------------------------------------------------------------------
-- Derived accessors and the re-entrancy guard.
-- ---------------------------------------------------------------------------

askHandlers :: (Monad m) => ScopeT d t h c m (Handlers m d t h c)
askHandlers = (\e -> e.handlers) <$> askEnv

askExportPrefix :: (Monad m) => ScopeT d t h c m BwdPath
askExportPrefix = (\e -> e.exportPrefix) <$> askEnv

{- | Run a critical section.  Re-entering while locked raises 'Locked' (matching
the OCaml mutex).
-}
withLock :: (Monad m) => ScopeT d t h c m a -> ScopeT d t h c m a
withLock body = do
  s <- getScope
  if s.locked
    then throw Locked
    else do
      putScope s{locked = True}
      r <- body
      s' <- getScope
      putScope s'{locked = False}
      pure r

-- ---------------------------------------------------------------------------
-- Name resolution
-- ---------------------------------------------------------------------------

-- | Look up a name in the visible namespace.
resolve :: (Monad m) => Path -> ScopeT d t h c m (Maybe (d, t))
resolve p = withLock $ do
  s <- getScope
  pure (Trie.findSingleton p s.visible)

-- ---------------------------------------------------------------------------
-- Inclusion (affects both namespaces)
-- ---------------------------------------------------------------------------

{- | Add a binding to both the visible and export namespaces.  @ctxVisible@ and
@ctxExport@ are the contexts for the @shadow@ effect on each merge.
-}
includeSingleton :: (Monad m) => Maybe c -> Maybe c -> (Path, (d, t)) -> ScopeT d t h c m ()
includeSingleton ctxVisible ctxExport (path, x) = withLock $ do
  hs <- askHandlers
  pfx <- askExportPrefix
  s <- getScope
  vis' <- liftBase (Modifier.unionSingleton hs ctxVisible Emp s.visible (path, x))
  exp' <- liftBase (Modifier.unionSingleton hs ctxExport pfx s.export (path, x))
  putScope s{visible = vis', export = exp'}

-- | Merge a subtree (after applying @modifier@) into both namespaces.
includeSubtree ::
  (Monad m) =>
  -- | context for the modifier
  Maybe c ->
  -- | context for the visible-namespace merge
  Maybe c ->
  -- | context for the export-namespace merge
  Maybe c ->
  -- | modifier applied before merging (use @Language.id@ for none)
  Language h ->
  (Path, Trie d t) ->
  ScopeT d t h c m ()
includeSubtree ctxModifier ctxVisible ctxExport modifier pns =
  withLock (unsafeIncludeSubtree ctxModifier ctxVisible ctxExport modifier pns)

-- The unlocked core of 'includeSubtree', also used by 'section' (which already
-- holds the lock).
unsafeIncludeSubtree ::
  (Monad m) =>
  Maybe c -> Maybe c -> Maybe c -> Language h -> (Path, Trie d t) -> ScopeT d t h c m ()
unsafeIncludeSubtree ctxModifier ctxVisible ctxExport modifier (path, ns) = do
  hs <- askHandlers
  pfx <- askExportPrefix
  s <- getScope
  ns' <- liftBase (Modifier.modify hs ctxModifier Emp modifier ns)
  vis' <- liftBase (Modifier.unionSubtree hs ctxVisible Emp s.visible (path, ns'))
  exp' <- liftBase (Modifier.unionSubtree hs ctxExport pfx s.export (path, ns'))
  putScope s{visible = vis', export = exp'}

-- ---------------------------------------------------------------------------
-- Importing (affects the visible namespace only)
-- ---------------------------------------------------------------------------

-- | Add a binding to the visible namespace only.
importSingleton :: (Monad m) => Maybe c -> (Path, (d, t)) -> ScopeT d t h c m ()
importSingleton ctxVisible (path, x) = withLock $ do
  hs <- askHandlers
  s <- getScope
  vis' <- liftBase (Modifier.unionSingleton hs ctxVisible Emp s.visible (path, x))
  putScope s{visible = vis'}

-- | Merge a subtree (after applying @modifier@) into the visible namespace only.
importSubtree ::
  (Monad m) =>
  Maybe c -> Maybe c -> Language h -> (Path, Trie d t) -> ScopeT d t h c m ()
importSubtree ctxModifier ctxVisible modifier (path, ns) = withLock $ do
  hs <- askHandlers
  s <- getScope
  ns' <- liftBase (Modifier.modify hs ctxModifier Emp modifier ns)
  vis' <- liftBase (Modifier.unionSubtree hs ctxVisible Emp s.visible (path, ns'))
  putScope s{visible = vis'}

-- ---------------------------------------------------------------------------
-- Modifying namespaces
-- ---------------------------------------------------------------------------

-- | Run a modifier on the visible namespace.
modifyVisible :: (Monad m) => Maybe c -> Language h -> ScopeT d t h c m ()
modifyVisible ctxVisible m = withLock $ do
  hs <- askHandlers
  s <- getScope
  vis' <- liftBase (Modifier.modify hs ctxVisible Emp m s.visible)
  putScope s{visible = vis'}

-- | Run a modifier on the export namespace.
modifyExport :: (Monad m) => Maybe c -> Language h -> ScopeT d t h c m ()
modifyExport ctxExport m = withLock $ do
  hs <- askHandlers
  pfx <- askExportPrefix
  s <- getScope
  exp' <- liftBase (Modifier.modify hs ctxExport pfx m s.export)
  putScope s{export = exp'}

-- ---------------------------------------------------------------------------
-- Exporting
-- ---------------------------------------------------------------------------

{- | Run a modifier on the visible namespace and merge the result into the
export namespace.
-}
exportVisible :: (Monad m) => Maybe c -> Maybe c -> Language h -> ScopeT d t h c m ()
exportVisible ctxModifier ctxExport m = withLock $ do
  hs <- askHandlers
  pfx <- askExportPrefix
  s <- getScope
  modified <- liftBase (Modifier.modify hs ctxModifier Emp m s.visible)
  exp' <- liftBase (Modifier.union hs ctxExport pfx s.export modified)
  putScope s{export = exp'}

-- | The current visible namespace (e.g. for auto-completion).
getVisible :: (Monad m) => ScopeT d t h c m (Trie d t)
getVisible = withLock ((\s -> s.visible) <$> getScope)

-- | The current export namespace (e.g. when wrapping up a compilation unit).
getExport :: (Monad m) => ScopeT d t h c m (Trie d t)
getExport = withLock ((\s -> s.export) <$> getScope)

-- ---------------------------------------------------------------------------
-- Sections
-- ---------------------------------------------------------------------------

{- | Start a nested scope, run @body@ in it, and merge its export namespace
(prefixed with @p@, after @modifier@) into both namespaces of the parent.
-}
section ::
  (Monad m) =>
  Maybe c -> Maybe c -> Maybe c -> Language h -> Path -> ScopeT d t h c m a -> ScopeT d t h c m a
section ctxModifier ctxVisible ctxExport modifier p body = withLock $ do
  hs <- askHandlers
  pfx <- askExportPrefix
  s <- getScope
  -- The child scope inherits the parent's visible namespace, starts with an
  -- empty export namespace, its export prefix extended by @p@, and is unlocked.
  (ans, childExport) <-
    liftBase $
      evalStateT
        ( runReaderT
            (unScopeT (do r <- body; e <- getExportRaw; pure (r, e)))
            (Env{exportPrefix = pfx <@ p, handlers = hs})
        )
        (ScopeState{visible = s.visible, export = Trie.empty, locked = False})
  unsafeIncludeSubtree ctxModifier ctxVisible ctxExport modifier (p, childExport)
  pure ans
 where
  getExportRaw = (\s' -> s'.export) <$> getScope

-- ---------------------------------------------------------------------------
-- Runners
-- ---------------------------------------------------------------------------

{- | Run a scope computation with the given handler bundle, an empty export
prefix, and an empty initial visible namespace.
-}
run :: (Monad m) => Handlers m d t h c -> ScopeT d t h c m a -> m a
run hs = runWith hs Emp Trie.empty

{- | Run a scope computation, also specifying the export prefix and the initial
visible namespace.
-}
runWith :: (Monad m) => Handlers m d t h c -> BwdPath -> Trie d t -> ScopeT d t h c m a -> m a
runWith hs pfx initVisible body =
  evalStateT
    (runReaderT (unScopeT body) (Env{exportPrefix = pfx, handlers = hs}))
    (ScopeState{visible = initVisible, export = Trie.empty, locked = False})

{- | Run @body@ with the current handler bundle transformed by @f@ (the OCaml
@try_with@).  Overriding a field replaces that handler for @body@; leaving it
alone keeps the enclosing one (\"re-perform\").  For example, to silence
@shadow@:

> tryWith (\hs -> hs { shadow = silence.shadow }) body
-}
tryWith ::
  (Monad m) =>
  (Handlers m d t h c -> Handlers m d t h c) -> ScopeT d t h c m a -> ScopeT d t h c m a
tryWith f = localEnv (\e -> e{handlers = f e.handlers})
