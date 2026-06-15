# TrajLens Web Viewer

Interactive React-based web application for visualizing TrajLens graphs.

## Features

- **Interactive Visualization**: Pan, zoom, and explore graphs using React Flow
- **Multiple Graph Types**: Support for all 4 graph types (G1-G4)
- **Node Inspection**: Click nodes to view detailed information
- **IGR Import**: Load `.igr.toml` files directly in the browser
- **Search & Filter**: Find specific nodes and relationships
- **Cost Breakdown**: View token usage and dollar costs
- **Responsive Layout**: Adapts to different screen sizes

## Quick Start

### Prerequisites

- Node.js 18+ and npm
- Optional: TrajLens CLI for generating IGR files

### Installation

```bash
# Install dependencies
npm install
```

### Development

```bash
# Start development server with hot reload
npm run dev

# Open browser to http://localhost:5173
```

### Production Build

```bash
# Build for production
npm run build

# Preview production build
npm run preview

# Output will be in dist/
```

### Optional: WASM Integration

For in-browser log parsing (experimental):

```bash
# Build WASM module from Rust code
npm run build:wasm

# This creates src/wasm/ with trajlens_wasm.js and .wasm files
```

## Project Structure

```
trajlens-web/
├── src/
│   ├── components/       # React components
│   │   ├── GraphView.tsx # Main graph visualization
│   │   ├── FileUpload.tsx # IGR file uploader
│   │   ├── NodeDetails.tsx # Node info panel
│   │   └── Controls.tsx  # Zoom, pan controls
│   ├── wasm/            # Generated WASM files (gitignored)
│   ├── App.tsx          # Main application
│   └── main.tsx         # Entry point
├── public/              # Static assets
├── index.html           # HTML template
├── package.json         # Dependencies
├── tsconfig.json        # TypeScript config
└── vite.config.ts       # Vite config
```

## Usage

### 1. Generate IGR Files

Use the TrajLens CLI to generate graph files:

```bash
# Parse log and build graphs
trajlens run agent.log -o output/

# This creates:
# - output/trajectory.json
# - output/activity-graph.igr.toml
# - output/cost-map.igr.toml
```

For G1/G2 (requires LLM):

```bash
# Generate Goal Tree
trajlens build-llm goal-tree output/trajectory.json \
  -o output/goal-tree.igr.toml --llm anthropic

# Generate Reasoning DAG  
trajlens build-llm reasoning-dag output/trajectory.json \
  -o output/reasoning-dag.igr.toml --llm anthropic
```

### 2. Load in Web Viewer

1. Start the dev server: `npm run dev`
2. Open http://localhost:5173
3. Click "Upload IGR File"
4. Select one of the `.igr.toml` files
5. Explore the graph interactively!

### 3. Navigation

- **Pan**: Click and drag on empty space
- **Zoom**: Mouse wheel or pinch gesture
- **Select Node**: Click on a node to see details
- **Reset View**: Click "Fit View" button
- **Search**: Use search box to find nodes by ID or label

## Graph Types

### G1: Goal Transition Tree

- **Nodes**: Goals with hierarchy levels
- **Edges**: Transitions (next, backtrack, sub-goal)
- **Layout**: Tree structure with root at top
- **Colors**: By goal type (explore, write, verify)

### G2: Reasoning Artifact DAG

- **Nodes**: Ground truths and insights
- **Edges**: Inference relationships (infers, contradicts, supersedes)
- **Layout**: DAG with source at top
- **Colors**: By node type and confidence

### G3: Activity Graph

- **Nodes**: File operations grouped hierarchically
- **Edges**: Sequential flow
- **Layout**: Hierarchical left-to-right
- **Colors**: By operation type (read, write, edit, run)

### G4: Cost Map

- **Nodes**: Cost breakdown by category/goal
- **Layout**: Treemap (area = cost)
- **Colors**: By category with intensity showing relative cost

## Configuration

Edit `vite.config.ts` to customize:

```typescript
export default defineConfig({
  server: {
    port: 5173,  // Change development port
    open: true,  // Auto-open browser
  },
  build: {
    outDir: 'dist',  // Output directory
    sourcemap: true, // Generate sourcemaps
  },
})
```

## Environment Variables

Create `.env` file for customization:

```bash
# Development server port
VITE_PORT=5173

# API endpoint (if using backend)
VITE_API_URL=http://localhost:8080

# Enable WASM console logging
VITE_WASM_LOG=false
```

## Dependencies

**Core:**
- React 18+ - UI framework
- TypeScript 5+ - Type safety
- Vite 5+ - Build tool and dev server

**Visualization:**
- @xyflow/react (React Flow) - Graph visualization library
- dagre - Graph layout algorithm

**Parsing:**
- @iarna/toml - TOML parser for IGR files

**Styling:**
- CSS Modules - Scoped styling

## Development

### Adding a New Graph Type

1. Create component in `src/components/graphs/`:

```typescript
// src/components/graphs/MyGraphView.tsx
export function MyGraphView({ graph }: { graph: MyGraph }) {
  // Convert graph to React Flow format
  const nodes = graph.nodes.map(node => ({
    id: node.node_id,
    position: { x: 0, y: 0 },
    data: { label: node.label },
  }));

  return <ReactFlow nodes={nodes} />;
}
```

2. Register in `src/App.tsx`:

```typescript
if (graphType === 'my_graph') {
  return <MyGraphView graph={parsed} />;
}
```

### Debugging

**Enable verbose logging:**

```bash
VITE_WASM_LOG=true npm run dev
```

**Check browser console** for:
- TOML parsing errors
- Graph layout issues
- WASM module errors

**Use React DevTools** for component inspection.

## Deployment

### Static Hosting

Deploy `dist/` folder to any static host:

```bash
# Build
npm run build

# Deploy to Netlify
netlify deploy --prod --dir=dist

# Deploy to Vercel
vercel --prod dist

# Deploy to GitHub Pages
# (requires gh-pages package)
npm run build && npx gh-pages -d dist
```

### Docker

```dockerfile
FROM node:18-alpine AS build
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM nginx:alpine
COPY --from=build /app/dist /usr/share/nginx/html
EXPOSE 80
```

```bash
docker build -t trajlens-web .
docker run -p 8080:80 trajlens-web
```

## Troubleshooting

### Port Already in Use

```bash
# Change port
VITE_PORT=3000 npm run dev

# Or kill existing process
lsof -ti:5173 | xargs kill -9
```

### TOML Parsing Fails

- Check IGR file is valid TOML
- Verify `graph_type` field is present
- Ensure all required fields exist (nodes, edges, etc.)

### Graphs Not Rendering

- Check browser console for errors
- Verify IGR file matches expected schema
- Try loading a different graph type
- Clear browser cache

### WASM Not Loading

- Rebuild WASM: `npm run build:wasm`
- Check `src/wasm/` contains `.wasm` and `.js` files
- Verify WASM feature is enabled in Rust build
- Check browser supports WASM

## Performance

**Optimizations:**

- **Lazy Loading**: Components loaded on demand
- **Virtual Scrolling**: Large graphs paginated
- **Memoization**: React.memo for expensive renders
- **Code Splitting**: Vite automatically chunks code

**Large Graphs** (1000+ nodes):

- Use "Fit View" sparingly (expensive)
- Disable animations: `animated={false}`
- Consider server-side layout pre-computation

## Browser Support

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+
- Opera 76+

**Required Features:**
- ES2020 JavaScript
- CSS Grid and Flexbox
- WebAssembly 1.0 (for WASM parsing)

## Contributing

1. Follow React and TypeScript best practices
2. Use functional components with hooks
3. Keep components small and focused
4. Add TypeScript types for all props
5. Test on multiple browsers

## License

MIT License - see [LICENSE](../LICENSE) for details.

## Links

- [TrajLens Main Repo](../)
- [React Flow Documentation](https://reactflow.dev/)
- [Vite Documentation](https://vitejs.dev/)
- [React Documentation](https://react.dev/)
