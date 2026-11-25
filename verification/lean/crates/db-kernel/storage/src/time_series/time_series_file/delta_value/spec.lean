import crates.«db-kernel».storage.src.time_series.time_series_file.delta_value.generated
open Aeneas Aeneas.Std Result
namespace delta_value_proof
open delta_value.delta_value

/-- A page is viewed by key, under the production invariant of unique keys.
Physical offsets and checksums are deliberately absent from logical equality. -/
def applyAt {K T : Type} [DecidableEq K] (page : K → Option T)
    (key : K) (change : Change T) (query : K) : Result (Option T) :=
  if query = key then apply (page query) change else ok (page query)

/-- Composition of the extracted replay gate and per-key application. The I/O
wrapper stamps the batch LSN only after successfully constructing the image. -/
def replay {T : Type} (lsn batch : U64) (old : Option T) (change : Change T) :
    Result (U64 × Option T) := do
  let needed ← replay_needed lsn batch
  if needed then
    let value ← apply old change
    ok (batch, value)
  else ok (lsn, old)
end delta_value_proof
