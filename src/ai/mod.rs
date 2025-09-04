use rand::distributions::{Distribution, Uniform};
use rand::Rng;

// Evolutionary neural net with variable hidden layers and sizes
// Input: 16 (fixed)
// Output: 5 (fixed)
//
// Topology: [INPUT] -> [Hidden1?] -> ... -> [HiddenK?] -> [OUTPUT]
// Each layer is fully-connected, ReLU activations for hidden, linear for output.

pub const INPUT_DIM: usize = 16;
pub const OUTPUT_DIM: usize = 5;

// Reasonable bounds for structure mutation
pub const MIN_HIDDEN_UNITS: usize = 8;
pub const MAX_HIDDEN_UNITS: usize = 40;
pub const MAX_HIDDEN_LAYERS: usize = 4;

// Layer has weights [out_dim x in_dim] row-major (o,i) and biases [out_dim]
#[derive(Clone)]
pub struct Layer {
    pub in_dim: usize,
    pub out_dim: usize,
    pub w: Vec<f32>,
    pub b: Vec<f32>,
}

impl Layer {
    pub fn new_random(in_dim: usize, out_dim: usize, scale: f32, rng: &mut impl Rng) -> Self {
        let u = Uniform::new(-scale, scale);
        let mut w = vec![0.0; out_dim * in_dim];
        for v in &mut w {
            *v = u.sample(rng);
        }
        let mut b = vec![0.0; out_dim];
        for v in &mut b {
            *v = u.sample(rng);
        }
        Self { in_dim, out_dim, w, b }
    }
}

#[derive(Clone)]
pub struct Net {
    pub layers: Vec<Layer>, // includes output layer as the last one
}

impl Net {
    // Start with 1 hidden layer, sometimes 2
    pub fn new_random(rng: &mut impl Rng) -> Self {
        let mut hidden = vec![rand_hidden_units(rng)];
        let coin = Uniform::new(0.0f32, 1.0f32).sample(rng);
        if coin < 0.25 {
            hidden.push(rand_hidden_units(rng));
        }
        Net::from_hidden_sizes(&hidden, rng)
    }

    pub fn from_hidden_sizes(sizes: &[usize], rng: &mut impl Rng) -> Self {
        let mut layers = Vec::new();
        let mut in_dim = INPUT_DIM;
        for &h in sizes {
            layers.push(Layer::new_random(in_dim, h, 0.1, rng));
            in_dim = h;
        }
        // output layer
        layers.push(Layer::new_random(in_dim, OUTPUT_DIM, 0.1, rng));
        Self { layers }
    }

    pub fn forward(&self, x: &[f32; INPUT_DIM]) -> [f32; OUTPUT_DIM] {
        // propagate through variable hidden layers
        let mut a: Vec<f32> = x.to_vec();
        for (li, layer) in self.layers.iter().enumerate() {
            let mut z = vec![0.0f32; layer.out_dim];
            for o in 0..layer.out_dim {
                let mut acc = layer.b[o];
                let row = &layer.w[o * layer.in_dim..(o + 1) * layer.in_dim];
                for i in 0..layer.in_dim {
                    acc += row[i] * a[i];
                }
                // hidden = ReLU, output (last) = linear
                if li + 1 == self.layers.len() {
                    z[o] = acc;
                } else {
                    z[o] = acc.max(0.0);
                }
            }
            a = z;
        }
        let mut out = [0.0f32; OUTPUT_DIM];
        for i in 0..OUTPUT_DIM {
            out[i] = a[i];
        }
        out
    }
}

// Genome operations

fn rand_hidden_units(rng: &mut impl Rng) -> usize {
    Uniform::new_inclusive(MIN_HIDDEN_UNITS, MAX_HIDDEN_UNITS).sample(rng)
}

// Cross over two parent nets into a child net; shapes may differ.
// Strategy:
// 1) Choose template parent (random).
// 2) Copy template structure.
// 3) Blend overlapping weights/biases with the other parent (averaging).
// 4) Mutate weights (noise).
// 5) Optional structure mutation: add neurons to a hidden layer, or add a hidden layer.
pub fn crossover_mutate(
    dad: &Net,
    mom: &Net,
    rng: &mut impl Rng,
    mutation_rate: f32,
    mutation_mag: f32,
    p_add_neurons: f32,
    p_add_layer: f32,
) -> Net {
    let coin = Uniform::new(0.0f32, 1.0f32).sample(rng);
    let template_is_dad = coin < 0.5;
    let (tpl, other) = if template_is_dad { (dad, mom) } else { (mom, dad) };

    // 1) Copy template
    let mut child = tpl.clone();

    // 2) Blend overlapping ranges
    for (lidx, child_layer) in child.layers.iter_mut().enumerate() {
        if lidx >= other.layers.len() {
            break;
        }
        let other_layer = &other.layers[lidx];

        let in_min = child_layer.in_dim.min(other_layer.in_dim);
        let out_min = child_layer.out_dim.min(other_layer.out_dim);

        // Weights averaging in the intersecting block
        for o in 0..out_min {
            for i in 0..in_min {
                let ci = o * child_layer.in_dim + i;
                let oi = o * other_layer.in_dim + i;
                child_layer.w[ci] = 0.5 * (child_layer.w[ci] + other_layer.w[oi]);
            }
        }
        // Bias averaging in the intersecting part
        for o in 0..out_min {
            child_layer.b[o] = 0.5 * (child_layer.b[o] + other_layer.b[o]);
        }
    }

    // 3) Mutate weights/bias (add noise per-value with rate)
    let u01 = Uniform::new(0.0f32, 1.0f32);
    let u_noise = Uniform::new(-mutation_mag, mutation_mag);
    for layer in &mut child.layers {
        for v in &mut layer.w {
            if u01.sample(rng) < mutation_rate {
                *v += u_noise.sample(rng);
            }
        }
        for v in &mut layer.b {
            if u01.sample(rng) < mutation_rate {
                *v += u_noise.sample(rng);
            }
        }
    }

    // 4) Structural mutation: add neurons (to one random hidden layer)
    if u01.sample(rng) < p_add_neurons {
        // pick a hidden layer index (not the last output layer)
        if child.layers.len() >= 2 {
            let hid_count = child.layers.len() - 1;
            let target = Uniform::new(0usize, hid_count).sample(rng);
            add_neurons(&mut child, target, rng);
        }
    }

    // 5) Structural mutation: add a new hidden layer (limit layers)
    if u01.sample(rng) < p_add_layer && child.layers.len() - 1 < MAX_HIDDEN_LAYERS {
        // position between layers: 0..(len-1). Insert before output
        let pos = if child.layers.len() <= 1 {
            0
        } else {
            Uniform::new(0usize, child.layers.len() - 1).sample(rng)
        };
        add_hidden_layer(&mut child, pos, rng);
    }

    child
}

// Adds neurons to hidden layer at index `hid_idx` (0..hidden_count-1).
// We must expand this layer's out_dim and next layer's in_dim accordingly.
fn add_neurons(net: &mut Net, hid_idx: usize, rng: &mut impl Rng) {
    let len_layers = net.layers.len();
    if len_layers < 2 {
        return;
    }
    if hid_idx + 1 >= len_layers {
        return;
    }

    // Obtain two disjoint mutable refs: current hidden layer and the next layer
    let (left, right) = net.layers.split_at_mut(hid_idx + 1);
    let l = &mut left[hid_idx];
    let next = &mut right[0];

    // How many to add
    let add = Uniform::new_inclusive(1usize, 4usize).sample(rng);
    let new_out = (l.out_dim + add).min(MAX_HIDDEN_UNITS);
    if new_out == l.out_dim {
        return;
    }
    // let _added = new_out - l.out_dim;

    // Expand l.w (out x in): add rows
    let mut new_w = vec![0.0f32; new_out * l.in_dim];
    // copy old rows
    for o in 0..l.out_dim {
        let src = &l.w[o * l.in_dim..(o + 1) * l.in_dim];
        let dst = &mut new_w[o * l.in_dim..(o + 1) * l.in_dim];
        dst.copy_from_slice(src);
    }
    // init new rows
    let u = Uniform::new(-0.1f32, 0.1f32);
    for o in l.out_dim..new_out {
        for i in 0..l.in_dim {
            new_w[o * l.in_dim + i] = u.sample(rng);
        }
    }
    l.w = new_w;

    // Expand biases
    let mut new_b = vec![0.0f32; new_out];
    new_b[..l.out_dim].copy_from_slice(&l.b);
    for v in &mut new_b[l.out_dim..] {
        *v = u.sample(rng);
    }
    l.b = new_b;
    l.out_dim = new_out;

    // Update next layer's in_dim and weights
    let old_in = next.in_dim;
    let new_in = l.out_dim;
    let mut next_w = vec![0.0f32; next.out_dim * new_in];
    // copy intersecting part
    let in_min = old_in.min(new_in);
    for o in 0..next.out_dim {
        for i in 0..in_min {
            let src = next.w[o * old_in + i];
            next_w[o * new_in + i] = src;
        }
        // init new columns
        for i in in_min..new_in {
            next_w[o * new_in + i] = u.sample(rng);
        }
    }
    next.w = next_w;
    next.in_dim = new_in;
}

// Insert a new hidden layer at position `pos` (0..hidden_count). Insert before the layer at `pos`.
// Rewire adjacent layers and reinit affected weights.
fn add_hidden_layer(net: &mut Net, pos: usize, rng: &mut impl Rng) {
    if net.layers.is_empty() {
        return;
    }
    let new_units = rand_hidden_units(rng);
    // The incoming size to this new layer
    let prev_in = if pos == 0 { INPUT_DIM } else { net.layers[pos - 1].out_dim };
    // Insert new layer between prev and the existing layer at `pos`
    let new_layer = Layer::new_random(prev_in, new_units, 0.1, rng);
    net.layers.insert(pos, new_layer);

    // Rewire the following layer (was at pos, now at pos+1) to accept new_units as input
    if pos + 1 < net.layers.len() {
        let next = &mut net.layers[pos + 1];
        let out = next.out_dim;
        let mut w = vec![0.0f32; out * new_units];
        let u = Uniform::new(-0.1f32, 0.1f32);
        for v in &mut w {
            *v = u.sample(rng);
        }
        next.w = w;
        next.in_dim = new_units;
    }
}