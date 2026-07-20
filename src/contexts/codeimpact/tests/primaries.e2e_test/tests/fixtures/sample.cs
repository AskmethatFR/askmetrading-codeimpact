public class Sample
{
    public int Compute(int x)
    {
        if (x > 0)
        {
            for (int i = 0; i < x; i++)
            {
                DoWork(i);
            }
        }
        return x;
    }

    private void DoWork(int i)
    {
    }
}
