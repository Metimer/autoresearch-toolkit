from workload import workload

assert workload()[0] == [n * 3 for n in range(150)]
