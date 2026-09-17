#nullable enable

using System.Text.Json;
using mududb.codec;
using Xunit;

namespace mududb.tests;

/// <summary>
/// Golden-frame interop proof: the C# SDK codec against the canonical MSSP v1
/// corpus produced by the Rust host codec
/// (`mudu_binding::codec::syscall_payload`).
///
/// The corpus is `crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin`
/// (generated once by the `generate_golden_v1_fixtures` test in the `testing`
/// crate). It packs 47 frames as consecutive segments, each prefixed with its
/// big-endian u32 byte length: for each of the 23 message kinds (in
/// discriminant order) one request frame followed by one ok response frame,
/// plus one trailing `get` UniError response frame.
///
/// The semantic expectations come from the JSON sidecar
/// `syscall_payload_v1_all.json` (the single source of truth, shared across
/// languages — nothing is hardcoded here): every frame is decoded and
/// compared against its `expect` object by <see cref="MsspCorpus.AssertVector"/>.
/// Because these frames are canonical encoder output, request frames
/// additionally pin the canonical encoder (the re-encoded decoded arguments
/// must equal the committed frame byte for byte) and ok responses go through
/// a decode → re-encode → decode semantic roundtrip; the err frame is
/// asserted field by field. This test only reads the fixture — it never
/// writes it.
/// </summary>
public class GoldenFrameCorpusTests
{
    private const int KindCount = 23;

    [Fact]
    public void GoldenCorpusMatchesCSharpCodecForAllKinds()
    {
        var segments = MsspCorpus.UnpackSegments(File.ReadAllBytes(MsspCorpus.FixturePath("syscall_payload_v1_all.bin")));
        using var sidecar = JsonDocument.Parse(File.ReadAllBytes(MsspCorpus.FixturePath("syscall_payload_v1_all.json")));
        var frames = sidecar.RootElement.GetProperty("frames");
        Assert.Equal(frames.GetArrayLength(), segments.Count);

        var seenKinds = new List<MessageKind>();
        var index = 0;
        foreach (var frame in frames.EnumerateArray())
        {
            Assert.Equal(index, frame.GetProperty("index").GetInt32());
            if (frame.GetProperty("direction").GetString() == "request")
            {
                seenKinds.Add((MessageKind)frame.GetProperty("message_kind").GetInt32());
            }

            MsspCorpus.AssertVector(segments[index], frame, canonical: true);
            index++;
        }

        Assert.Equal(KindCount, seenKinds.Distinct().Count());
    }
}
