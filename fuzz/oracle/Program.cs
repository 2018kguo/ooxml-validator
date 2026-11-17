// The oracle obstat is checked against: Microsoft's Open XML SDK validator
// (FileFormatVersions.Microsoft365). Reads .docx paths on stdin, one per line;
// writes one JSON object per path: {Path, Ok, OpenError, Errors: [{Id, Part,
// Description, XPath, Node}]}. Every error is kept. Before the SDK opens the
// package, each .xml and .rels entry must load as XML (XmlReader with DTDs
// prohibited); the first that does not is the only error, Id "xml_parse".
using System.IO.Compression;
using System.Text.Json;
using System.Xml;
using System.Xml.Linq;
using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Validation;

var validator = new OpenXmlValidator(FileFormatVersions.Microsoft365) { MaxNumberOfErrors = 0 };
string? line;
while ((line = Console.In.ReadLine()) != null)
{
    var path = line.Trim();
    if (path.Length == 0) continue;
    Console.WriteLine(JsonSerializer.Serialize(Validate(path)));
    Console.Out.Flush();
}

Dictionary<string, object?> Validate(string path)
{
    var errors = new List<object>();
    var result = new Dictionary<string, object?> { ["Path"] = path, ["Ok"] = false, ["OpenError"] = null, ["Errors"] = errors };
    try
    {
        var bytes = File.ReadAllBytes(path);
        using (var zip = new ZipArchive(new MemoryStream(bytes), ZipArchiveMode.Read))
        {
            foreach (var entry in zip.Entries)
            {
                var name = entry.FullName;
                if (!name.EndsWith(".xml", StringComparison.OrdinalIgnoreCase) && !name.EndsWith(".rels", StringComparison.OrdinalIgnoreCase)) continue;
                using var stream = entry.Open();
                using var reader = XmlReader.Create(stream, new XmlReaderSettings { DtdProcessing = DtdProcessing.Prohibit, XmlResolver = null });
                try { XDocument.Load(reader, LoadOptions.PreserveWhitespace); }
                catch (XmlException e)
                {
                    result["OpenError"] = e.Message;
                    errors.Add(new { Id = "xml_parse", Part = "/" + name, Description = e.Message, XPath = (string?)null, Node = (string?)null });
                    return result;
                }
            }
        }
        using var doc = WordprocessingDocument.Open(new MemoryStream(bytes), false);
        foreach (var e in validator.Validate(doc))
        {
            errors.Add(new { e.Id, Part = e.Part?.Uri.ToString(), e.Description, e.Path?.XPath, Node = e.Node?.LocalName });
        }
        result["Ok"] = errors.Count == 0;
    }
    catch (Exception e)
    {
        result["OpenError"] = $"{e.GetType().Name}: {e.Message}";
    }
    return result;
}
