// Shared walk configuration - SINGLE SOURCE OF TRUTH
// Both sources.html and neural_walks_compare.html use this file

(function() {
    // Named mappings
    window.MAPPINGS = {
        'Optimal':   [0,1,2,3,4,5,6,7,10,9,8,11],
        'Spiral':    [0,2,4,6,8,10,1,3,5,7,9,11],
        'Identity':  [0,1,2,3,4,5,6,7,8,9,10,11],
        'LCG':       [3,7,11,10,4,0,9,6,5,1,2,8],
        'Stock-opt': [1,0,2,4,10,5,6,9,8,7,3,11],
    };

    // File -> { walk type, mapping name, category info }
    // base4 files are excluded from this gallery
    window.FILE_INFO = {
        // Animals
        'amazon_comparison_data.js':   { mapping: 'Optimal', cat: 'animals', sub: 'Birds' },
        'animals_walk_data.js':        { mapping: 'Optimal', cat: 'animals', sub: 'Animals' },
        'frogs_walk_data.js':          { mapping: 'Optimal', cat: 'animals', sub: 'Frogs' },
        'whales_walk_data.js':         { mapping: 'Optimal', cat: 'animals', sub: 'Whales' },
        // Environment (ESC-50 dataset)
        'environment_walk_data.js':    { mapping: 'Optimal', cat: 'environment', sub: 'Environment' },
        // DNA
        'plant_dna_walk_data.js':      { mapping: 'Optimal', cat: 'dna', sub: 'Plants' },
        'covid_walks_best.js':         { mapping: 'Optimal', cat: 'dna', sub: 'Coronaviruses' },
        'other_virus_walks.js':        { mapping: 'Optimal', cat: 'dna', sub: 'Other Viruses' },
        'dna_walk_data.js':            { mapping: 'Identity', cat: 'dna', sub: 'Coronaviruses' },
        // Finance
        'stock_walk_optimized.js':     { mapping: 'Stock-opt', cat: 'finance', sub: 'Indices' },
        // Cosmos/Signals
        'cosmos_real_walk_data.js':    { mapping: 'Identity', cat: 'cosmos', sub: 'Cosmos' },
        // Music (real scores)
        'composers_walk_data.js':      { mapping: 'Identity', cat: 'audio', sub: 'Composers' },
        'real_birdsong_walk_data.js':  { mapping: 'Optimal', cat: 'audio', sub: 'Birdsong' },
        // Mathematical abstractions (computed, explicitly not empirical)
        'pi_walk_data.js':             { mapping: 'Identity', cat: 'math', sub: 'Constants' },
        'mandelbrot_walk_data.js':     { mapping: 'Identity', cat: 'math', sub: 'Fractals' },
        'fractal_walk_data.js':        { mapping: 'Identity', cat: 'math', sub: 'Fractals' },
    };

    window.FILE_SOURCES = {
        'amazon_comparison_data.js': [
            { label: 'Archive.org birds', url: 'https://archive.org/details/various-bird-sounds' },
            { label: 'Archive.org indigenous music', url: 'https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr' },
        ],
        'animals_walk_data.js': [
            { label: 'ESC-50', url: 'https://github.com/karolpiczak/ESC-50' },
        ],
        'frogs_walk_data.js': [
            { label: 'ESC-50', url: 'https://github.com/karolpiczak/ESC-50' },
        ],
        'environment_walk_data.js': [
            { label: 'ESC-50', url: 'https://github.com/karolpiczak/ESC-50' },
        ],
        'whales_walk_data.js': [
            { label: 'NOAA PMEL Acoustics', url: 'https://www.pmel.noaa.gov/acoustics/whales/sounds/' },
            { label: 'NOAA Fisheries', url: 'https://www.fisheries.noaa.gov/national/sounds-ocean' },
            { label: 'Internet Archive', url: 'https://archive.org/details/whale-songs-whale-sound-effects' },
        ],
        'plant_dna_walk_data.js': [
            { label: 'NCBI Nucleotide', url: 'https://www.ncbi.nlm.nih.gov/nuccore/' },
        ],
        'covid_walks_best.js': [
            { label: 'NCBI Nucleotide', url: 'https://www.ncbi.nlm.nih.gov/nuccore/' },
        ],
        'other_virus_walks.js': [
            { label: 'NCBI Nucleotide', url: 'https://www.ncbi.nlm.nih.gov/nuccore/' },
        ],
        'dna_walk_data.js': [
            { label: 'NCBI Nucleotide', url: 'https://www.ncbi.nlm.nih.gov/nuccore/' },
        ],
        'cosmos_real_walk_data.js': [
            { label: 'LIGO Open Data', url: 'https://www.gw-openscience.org/' },
        ],
        'real_birdsong_walk_data.js': [
            { label: 'Xeno-canto', url: 'https://xeno-canto.org/' },
        ],
        'stock_walk_optimized.js': [
            { label: 'Yahoo Finance', url: 'https://finance.yahoo.com/' },
        ],
        'composers_walk_data.js': [
            { label: 'Bach BWV 846/847/772/565 (IMSLP)', url: 'https://imslp.org/wiki/Category:Bach,_Johann_Sebastian' },
            { label: 'Beethoven WoO 59, Op.27/125/67/13 (IMSLP)', url: 'https://imslp.org/wiki/Category:Beethoven,_Ludwig_van' },
            { label: 'Schoenberg Op.25/31/37/4/21 (IMSLP)', url: 'https://imslp.org/wiki/Category:Schoenberg,_Arnold' },
        ],
        'pi_walk_data.js': [
            { label: 'Pi digits reference', url: 'https://oeis.org/A000796' },
        ],
        'mandelbrot_walk_data.js': [
            { label: 'Mandelbrot set reference', url: 'https://en.wikipedia.org/wiki/Mandelbrot_set' },
        ],
        'fractal_walk_data.js': [
            { label: 'Fractal sequence references', url: 'https://oeis.org/' },
        ],
    };

    // Data provenance classification:
    // - empirical: real-world sourced data (must have source links)
    // - mathematical: computed abstractions (must be labeled as computed)
    // - pcpri: intentional synthetic PCPRI data
    window.FILE_PROVENANCE = {
        'amazon_comparison_data.js': 'empirical',
        'animals_walk_data.js': 'empirical',
        'frogs_walk_data.js': 'empirical',
        'whales_walk_data.js': 'empirical',
        'environment_walk_data.js': 'empirical',
        'plant_dna_walk_data.js': 'empirical',
        'covid_walks_best.js': 'empirical',
        'other_virus_walks.js': 'empirical',
        'dna_walk_data.js': 'empirical',
        'cosmos_real_walk_data.js': 'empirical',
        'real_birdsong_walk_data.js': 'empirical',
        'stock_walk_optimized.js': 'empirical',
        'composers_walk_data.js': 'empirical',
        'pi_walk_data.js': 'mathematical',
        'mandelbrot_walk_data.js': 'mathematical',
        'fractal_walk_data.js': 'mathematical',
    };

    window.WALK_SOURCE_OVERRIDES = {
        'stock_walk_optimized.js::NASDAQ': [{ label: 'NASDAQ (^IXIC)', url: 'https://finance.yahoo.com/quote/%5EIXIC/history' }],
        'stock_walk_optimized.js::DOW': [{ label: 'DOW (^DJI)', url: 'https://finance.yahoo.com/quote/%5EDJI/history' }],
        'stock_walk_optimized.js::SP500': [{ label: 'S&P 500 (^GSPC)', url: 'https://finance.yahoo.com/quote/%5EGSPC/history' }],
        'covid_walks_best.js::SARS_CoV_2_Wuhan': [{ label: 'NC_045512.2', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_045512.2' }],
        'covid_walks_best.js::SARS_CoV_1': [{ label: 'NC_004718.3', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_004718.3' }],
        'covid_walks_best.js::MERS_CoV': [{ label: 'NC_019843.3', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_019843.3' }],
        'covid_walks_best.js::HCoV_229E': [{ label: 'NC_002645.1', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_002645.1' }],
        'covid_walks_best.js::HCoV_OC43': [{ label: 'NC_006213.1', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_006213.1' }],
        'plant_dna_walk_data.js::Arabidopsis thaliana chloroplast': [{ label: 'NC_000932.1', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_000932.1' }],
        'plant_dna_walk_data.js::Oryza sativa Japonica Group plastid': [{ label: 'NC_001320.1', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_001320.1' }],
        'plant_dna_walk_data.js::Zea mays chloroplast': [{ label: 'NC_001666.2', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_001666.2' }],
        'plant_dna_walk_data.js::Triticum aestivum chloroplast': [{ label: 'NC_002762.1', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_002762.1' }],
        'plant_dna_walk_data.js::Nicotiana tabacum plastid': [{ label: 'NC_001879.2', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_001879.2' }],
        'plant_dna_walk_data.js::Spinacia oleracea plastid': [{ label: 'NC_002202.1', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_002202.1' }],
        'plant_dna_walk_data.js::Picea abies chloroplast': [{ label: 'NC_021456.1', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_021456.1' }],
        'plant_dna_walk_data.js::Ginkgo biloba chloroplast': [{ label: 'NC_016986.1', url: 'https://www.ncbi.nlm.nih.gov/nuccore/NC_016986.1' }],
        // Amazon birds — only bird source, not indigenous music
        'amazon_comparison_data.js::Amazon Jungle Morning (Birds)': [{ label: 'Archive.org birds', url: 'https://archive.org/details/various-bird-sounds' }],
        'amazon_comparison_data.js::Amazon Forest Birds': [{ label: 'Archive.org birds', url: 'https://archive.org/details/various-bird-sounds' }],
        // Amazon indigenous music — only indigenous source
        'amazon_comparison_data.js::Karaja - Solo Song Man': [{ label: 'Archive.org indigenous music', url: 'https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr' }],
        'amazon_comparison_data.js::Karaja - Sacred Dance Aruana': [{ label: 'Archive.org indigenous music', url: 'https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr' }],
        'amazon_comparison_data.js::Karaja - Boys Girls Choir': [{ label: 'Archive.org indigenous music', url: 'https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr' }],
        'amazon_comparison_data.js::Kraho - Reversal Singing': [{ label: 'Archive.org indigenous music', url: 'https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr' }],
        'amazon_comparison_data.js::Kraho - Hoof Rattle': [{ label: 'Archive.org indigenous music', url: 'https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr' }],
        'amazon_comparison_data.js::Suya - Shukarramae Solo': [{ label: 'Archive.org indigenous music', url: 'https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr' }],
        'amazon_comparison_data.js::Random (baseline)': [],
    };

    // Base-4 files to EXCLUDE from the base-12 gallery
    window.BASE4_FILES = new Set([
        'dna_walk_data.js',
    ]);

    // Per-walk mapping overrides
    window.WALK_MAPPING_OVERRIDES = {};

    // Per-walk subcategory overrides
    window.WALK_SUB_OVERRIDES = {
        // Animals subcategories
        'animals_walk_data.js::Insects (Buzzing)': 'Insects',
        'animals_walk_data.js::Frog': 'Amphibians',
        'animals_walk_data.js::Crow': 'Birdsong',
        'animals_walk_data.js::Dog': 'Mammals',
        'animals_walk_data.js::Cat': 'Mammals',
        // Crickets and Chirping Birds are in animals file
        'animals_walk_data.js::Crickets': 'Insects',
        'animals_walk_data.js::Chirping Birds': 'Birdsong',
        // Amazon birds -> Birdsong (merged with real_birdsong_walk_data.js)
        'amazon_comparison_data.js::Amazon Jungle Morning (Birds)': 'Birdsong',
        'amazon_comparison_data.js::Amazon Forest Birds': 'Birdsong',
        'amazon_comparison_data.js::Random (baseline)': 'Birdsong',
        // Indigenous music from amazon_comparison
        'amazon_comparison_data.js::Karaja - Solo Song Man': 'Indigenous People of the Amazon Rainforest',
        'amazon_comparison_data.js::Karaja - Sacred Dance Aruana': 'Indigenous People of the Amazon Rainforest',
        'amazon_comparison_data.js::Karaja - Boys Girls Choir': 'Indigenous People of the Amazon Rainforest',
        'amazon_comparison_data.js::Kraho - Reversal Singing': 'Indigenous People of the Amazon Rainforest',
        'amazon_comparison_data.js::Kraho - Hoof Rattle': 'Indigenous People of the Amazon Rainforest',
        'amazon_comparison_data.js::Suya - Shukarramae Solo': 'Indigenous People of the Amazon Rainforest',
        'amazon_comparison_data.js::Tukuna - Lullaby': 'Indigenous People of the Amazon Rainforest',
    };

    // Category overrides
    window.WALK_CAT_OVERRIDES = {
        // Environment sounds from ESC-50 (REAL)
        'environment_walk_data.js::Environment: Sea Waves': 'environment',
        'environment_walk_data.js::Environment: Wind': 'environment',
        'environment_walk_data.js::Environment: Thunderstorm': 'environment',
        'environment_walk_data.js::Environment: Crackling Fire': 'environment',
        'environment_walk_data.js::Environment: Rain': 'environment',
        // Birds -> Audio category (merged with Birdsong)
        'animals_walk_data.js::Crow': 'audio',
        'animals_walk_data.js::Chirping Birds': 'audio',
        'amazon_comparison_data.js::Amazon Jungle Morning (Birds)': 'audio',
        'amazon_comparison_data.js::Amazon Forest Birds': 'audio',
        'amazon_comparison_data.js::Random (baseline)': 'audio',
        // Indigenous music -> Audio category
        'amazon_comparison_data.js::Karaja - Solo Song Man': 'audio',
        'amazon_comparison_data.js::Karaja - Sacred Dance Aruana': 'audio',
        'amazon_comparison_data.js::Karaja - Boys Girls Choir': 'audio',
        'amazon_comparison_data.js::Kraho - Reversal Singing': 'audio',
        'amazon_comparison_data.js::Kraho - Hoof Rattle': 'audio',
        'amazon_comparison_data.js::Suya - Shukarramae Solo': 'audio',
        'amazon_comparison_data.js::Tukuna - Lullaby': 'audio',
    };

    // File loading info (for neural_walks_compare.html)
    window.FILE_LOADING = {
        'amazon_comparison_data.js': { global: 'AMAZON_COMPARISON_DATA', mode: 'points' },
        'animals_walk_data.js': { global: 'ANIMALS_WALK_DATA', mode: 'points' },
        'composers_walk_data.js': { global: 'COMPOSERS_WALK_DATA', mode: 'points' },
        'cosmos_real_walk_data.js': { global: 'COSMOS_REAL_WALK_DATA', mode: 'points' },
        'covid_walks_best.js': { global: 'COVID_WALKS', mode: 'walks_path' },
        'dna_walk_data.js': { global: 'DNA_WALK_DATA', mode: 'points' },
        'environment_walk_data.js': { global: 'ENVIRONMENT_WALK_DATA', mode: 'points' },
        'fractal_walk_data.js': { global: 'FRACTAL_WALK_DATA', mode: 'points' },
        'frogs_walk_data.js': { global: 'FROGS_WALK_DATA', mode: 'points' },
        'mandelbrot_walk_data.js': { global: 'MANDELBROT_WALK_DATA', mode: 'points' },
        'other_virus_walks.js': { global: 'OTHER_VIRUS_WALKS', mode: 'points' },
        'pi_walk_data.js': { global: 'PI_WALK_DATA', mode: 'points' },
        'plant_dna_walk_data.js': { global: 'PLANT_DNA_WALK_DATA', mode: 'points' },
        'real_birdsong_walk_data.js': { global: 'REAL_BIRDSONG_WALK_DATA', mode: 'points' },
        'stock_walk_optimized.js': { global: 'STOCK_WALK_OPTIMIZED', mode: 'points' },
        'whales_walk_data.js': { global: 'WHALES_WALK_DATA', mode: 'points' },
    };

    window.getFileLoading = function(filename) {
        return window.FILE_LOADING[filename] || null;
    };

    // Category display names
    window.CAT_NAMES = {
        audio: 'Audio',
        languages: 'Languages',
        animals: 'Animals',
        environment: 'Environment',
        dna: 'DNA',
        cosmos: 'Cosmos',
        finance: 'Finance',
        math: 'Math',
        pcpri: 'Phase-Coherent Pseudorandom Input',
        games: 'Games'
    };

    // Helper functions
    window.getFileInfo = function(filename) {
        return window.FILE_INFO[filename] || null;
    };

    window.getWalkSources = function(file, walkName) {
        const key = file + '::' + walkName;
        if (window.WALK_SOURCE_OVERRIDES[key]) return window.WALK_SOURCE_OVERRIDES[key];
        return window.FILE_SOURCES[file] || [];
    };

    window.getWalkProvenance = function(file) {
        return window.FILE_PROVENANCE[file] || 'unknown';
    };

    window.getWalkCat = function(file, walkName) {
        const key = file + '::' + walkName;
        if (window.WALK_CAT_OVERRIDES[key]) return window.WALK_CAT_OVERRIDES[key];
        const info = window.FILE_INFO[file];
        return info ? info.cat : 'other';
    };

    window.getWalkSub = function(file, walkName) {
        const key = file + '::' + walkName;
        if (window.WALK_SUB_OVERRIDES[key]) return window.WALK_SUB_OVERRIDES[key];
        // Use FILE_INFO.sub as primary source
        const info = window.FILE_INFO[file];
        return info ? info.sub : 'Other';
    };

    window.getWalkMapping = function(file, walkName) {
        const key = file + '::' + walkName;
        if (window.WALK_MAPPING_OVERRIDES[key]) return window.WALK_MAPPING_OVERRIDES[key];
        const info = window.FILE_INFO[file];
        return info ? info.mapping : 'Unknown';
    };

    // No HIDDEN_WALKS - if data shouldn't be shown, don't put it in FILE_INFO
    window.HIDDEN_WALKS = new Set();

    // No isBaseline filter needed - data files are clean
    window.isBaseline = function(name) { return false; };

    window.provenanceLabel = function(kind) {
        if (kind === 'empirical') return 'Empirical (real source)';
        if (kind === 'mathematical') return 'Mathematical (computed)';
        if (kind === 'pcpri') return 'PCPRI synthetic (intentional)';
        return 'Unclassified';
    };

    window.mappingTagClass = function(mapping) {
        if (mapping === 'Optimal') return 'optimal';
        if (mapping === 'Spiral') return 'spiral';
        if (mapping === 'Identity') return 'identity';
        return 'other';
    };
})();
