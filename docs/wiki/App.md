# App
Kod: `src/app.rs`. Tarama ve temizlik `std::thread` + `mpsc`; AI istekleri tokio runtime'da. `poll_jobs` her karede kanalları boşaltır. Satır işlemleri tablo çizerken `Act` listesine toplanıp sonra uygulanır (borrow).
GOTCHA: eframe 0.37'de `App::ui(&mut Ui)` ana giriş (`update` değil), `Panel::left(..).show(ui, ..)`, `Id::unique` (`Id::new` deprecated).
