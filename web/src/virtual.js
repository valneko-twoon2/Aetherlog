// Row offsets for a long list whose rows have different heights (wrapped lines, day banners).
// Every row starts at an estimated height; measured heights are kept as deltas in a Fenwick
// tree, so offsets and totals cost O(log n) even for hundreds of thousands of rows.
export class HeightIndex {
  constructor(count, estimate) {
    this.count = count
    this.estimate = estimate
    this.tree = new Float64Array(count + 1)
    this.delta = new Float64Array(count)
  }

  get(i) {
    return this.estimate + this.delta[i]
  }

  set(i, height) {
    const d = height - this.estimate - this.delta[i]
    if (!d) return
    this.delta[i] += d
    for (let x = i + 1; x <= this.count; x += x & -x) this.tree[x] += d
  }

  // Top of row i, relative to the top of the list.
  offset(i) {
    let sum = 0
    for (let x = i; x > 0; x -= x & -x) sum += this.tree[x]
    return i * this.estimate + sum
  }

  total() {
    return this.offset(this.count)
  }

  // The row that covers vertical position y.
  find(y) {
    let lo = 0
    let hi = this.count - 1
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1
      if (this.offset(mid) <= y) lo = mid
      else hi = mid - 1
    }
    return lo
  }
}
