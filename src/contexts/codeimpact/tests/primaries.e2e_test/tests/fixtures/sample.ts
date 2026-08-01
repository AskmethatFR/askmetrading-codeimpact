export class Sample {
  compute(x: number): number {
    if (x > 0) {
      for (let i = 0; i < x; i++) {
        fs.readFileSync(String(i));
      }
    }
    const label = x > 0 ? "positive" : "non-positive";
    return x;
  }
}
