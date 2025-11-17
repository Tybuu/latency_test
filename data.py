import numpy as np
def percent_diff(a, b):
    return (abs(a - b) / b) * 100

data = np.loadtxt('./modern.csv', delimiter=',')
m_tran = np.sum(data[:, 2])
m_mean = np.mean(data[:, 1])
m_dev = np.std(data[:, 1])
m_perc = np.percentile(data[:, 1], 99)
print(f"----Embassy's interrupt executor data----")
print(f"Retranmissions {np.sum(data[:, 2])}")
print(f"Mean {np.mean(data[:, 1])}")
print(f"Deviation {np.std(data[:, 1])}")
print(f"99th percentile {np.percentile(data[:, 1], 99)}")

trad_data = np.loadtxt('./trad.csv', delimiter=',')
t_tran = np.sum(trad_data[:, 2])
t_mean = np.mean(trad_data[:, 1])
t_dev = np.std(trad_data[:, 1])
t_perc = np.percentile(trad_data[:, 1], 99)
print(f"----Tradational interrupt handler trad_data----")
print(f"Retranmissions {np.sum(trad_data[:, 2])}")
print(f"Mean {np.mean(trad_data[:, 1])}")
print(f"Deviation {np.std(trad_data[:, 1])}")
print(f"99th percentile {np.percentile(trad_data[:, 1], 99)}")

print(f"----Difference----")
print(f"Retranmissions difference {m_tran - t_tran} | Percent Diff {percent_diff(m_tran, t_tran)}")
print(f"Mean difference {np.mean(data[:, 1]) - np.mean(trad_data[:, 1])} | Percent Diff {percent_diff(m_mean, t_mean)}")
print(f"Deviation {np.std(data[:, 1]) - np.std(trad_data[:, 1])} | Percent Diff {percent_diff(m_dev, t_dev)}")
print(f"99th percentile {np.percentile(data[:, 1], 99) - np.percentile(trad_data[:, 1], 99)} | Percent Diff {percent_diff(m_perc, t_perc)}")

