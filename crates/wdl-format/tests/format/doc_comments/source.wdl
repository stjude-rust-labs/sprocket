## Read groups are defined in the SAM spec:
##
## - `ID`: Read group identifier. Each Read Group must have a unique ID.
##     The value of ID is used in the RG tags of alignment records.
## - `BC`: Barcode sequence identifying the sample or library. This value is the
##     expected barcode bases as read by the sequencing machine in the absence
##     of errors. If there are several barcodes for the sample/library
##     (e.g., one on each end of the template), the recommended implementation
##     concatenates all the barcodes separating them with hyphens (`-`).
## - `CN`: Name of sequencing center producing the read.
## - `DS`: Description.
## - `DT`: Date the run was produced (ISO8601 date or date/time).
## - `FO`: Flow order. The array of nucleotide bases that correspond to the nucleotides
##     used for each flow of each read. Multi-base flows are encoded in IUPAC format,
##     and non-nucleotide flows by various other characters.
##     Format: `/\\*|[ACMGRSVTWYHKDBN]+/`
## - `KS`: The array of nucleotide bases that correspond to the key sequence of each read.
## - `LB`: Library.
## - `PG`: Programs used for processing the read group.
## - `PI`: Predicted median insert size, rounded to the nearest integer.
## - `PL`: Platform/technology used to produce the reads.
##     Valid values: CAPILLARY, DNBSEQ (MGI/BGI), ELEMENT, HELICOS, ILLUMINA, IONTORRENT,
##     LS454, ONT (Oxford Nanopore), PACBIO (Pacific Biosciences), SINGULAR, SOLID,
##     and ULTIMA. This field should be omitted when the technology is not in this list
##     (though the PM field may still be present in this case) or is unknown.
## - `PM`: Platform model. Free-form text providing further details of the
##     platform/technology used.
## - `PU`: Platform unit (e.g., flowcell-barcode.lane for Illumina or slide
##     for SOLiD). Unique identifier.
## - `SM`: Sample. Use pool name where a pool is being sequenced.
##
## An example input JSON entry for `read_group` might look like this:
## ```json
## {
##     "read_group": {
##         "ID": "rg1",
##         "PI": 150,
##         "PL": "ILLUMINA",
##         "SM": "Sample",
##         "LB": "Sample"
##     }
## }
## ```
## # Header
## text without blank after header
version 1.3

## short description
##
## text
workflow foo {
## unindented doc comment that should get bumped out
    input {
            ## too much indentation here!
        String bar
    }
}