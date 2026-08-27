interface View {
  is_true(): boolean
}

interface Transaction extends View {
  make_true(): void
}
