const {
    renderNode
} = window.__REACT_AST_DEBUG_RENDERER__;

// Import reveal.js components from window
const { Deck, Slide } = window.RevealReact || {};

export const Ast = ({ ast, onNavigateToDocument, setAst }) => {
    return (
        <Deck
            config={{

                controls: true,
                progress: true,
                center: true,
                hash: false,
                transition: 'slide',
                backgroundTransition: 'fade',
                keyboard: false,
            }}
        >
            {ast.blocks.map((block, i) => (
                <Slide key={i}>
                    {renderNode({
                        node: block,
                        setLocalAst: (newBlock) => {
                            const newBlocks = [...ast.blocks];
                            newBlocks[i] = newBlock;
                            setAst({ ...ast, blocks: newBlocks });
                        },
                        onNavigateToDocument
                    }, block.t)}
                </Slide>
            ))}
        </Deck>
    );
};