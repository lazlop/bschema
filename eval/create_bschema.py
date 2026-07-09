from rdflib import Graph
from bschema_rs import create_bschema, bind_prefixes
import os
import csv
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
from time import time
import gc

def remove_triples(g, g2):
    for triple in g2:
        g.remove(triple)


def get_graphs(directory_path):
    for file_name in os.listdir(directory_path):
        if file_name.endswith(".ttl"):
            file_path = os.path.join(directory_path, file_name)
            print(f"Processing file: {file_name}")
            g = Graph(store = "Oxigraph")
            g.parse(file_path, format = 'ttl')
            yield file_name, g


def plot_stats(csv_path):
    """Plots runtime and compression vs. original graph size, one line per
    threshold, mirroring plot_bschema.py."""
    df = pd.read_csv(csv_path)

    def threshold_label(t):
        if pd.isna(t):
            return 'full'
        return str(t)

    df['threshold_label'] = df['threshold'].apply(threshold_label)

    threshold_order = ['0.0', '0.3', '0.5', '0.7', 'full']
    display_labels = {'0.0': '0.0', '0.3': '0.3', '0.5': '0.5', '0.7': '0.7', 'full': '1.0'}
    colors = plt.cm.viridis(np.linspace(0, 0.85, len(threshold_order)))

    fig, ax = plt.subplots(figsize=(8, 4))
    for color, label in zip(colors, threshold_order):
        subset = df[df['threshold_label'] == label].sort_values('graph_length')
        ax.plot(
            subset['graph_length'],
            subset['runtime'],
            marker='o',
            color=color,
            linewidth=2,
            markersize=6,
            label=f'τ = {display_labels[label]}',
        )

    ax.set_xscale('log')
    ax.set_yscale('log')
    ax.set_xlabel('Original Graph Size (triples)', fontsize=12)
    ax.set_ylabel('Runtime (seconds)', fontsize=12)
    ax.set_title('BSchema Runtime vs. Original Graph Size', fontsize=14, fontweight='bold')
    ax.legend(fontsize=10)
    ax.grid(True, alpha=0.3, which='both')

    plt.tight_layout()
    plt.savefig('bschema/bschema_runtime.png', dpi=300, bbox_inches='tight')
    plt.close(fig)
    print("Saved to bschema/bschema_runtime.png")

    fig, ax = plt.subplots(figsize=(8, 4))
    all_lengths = df['graph_length'].unique()
    xref = np.array([all_lengths.min(), all_lengths.max()])
    ax.plot(xref, xref, 'k--', linewidth=1, alpha=0.4, label='No Compression')

    for color, label in zip(colors, threshold_order):
        subset = df[df['threshold_label'] == label].sort_values('graph_length')
        ax.plot(
            subset['graph_length'],
            subset['bschema_length'],
            marker='o',
            color=color,
            linewidth=2,
            markersize=6,
            label=f'τ = {display_labels[label]}',
        )

    ax.set_xscale('log')
    ax.set_yscale('log')
    ax.set_xlabel('Original Graph Size (triples)', fontsize=12)
    ax.set_ylabel('BSchema Size (triples)', fontsize=12)
    ax.set_title('BSchema Compression vs. Original Graph Size', fontsize=14, fontweight='bold')
    ax.legend(fontsize=10)
    ax.grid(True, alpha=0.3, which='both')

    plt.tight_layout()
    plt.savefig('bschema/bschema_size_comparison.png', dpi=300, bbox_inches='tight')
    plt.close(fig)
    print("Saved to bschema/bschema_size_comparison.png")


if __name__ == "__main__":
    directory_path = "without-ontology"
    thresholds = [0, 0.3, 0.5, 0.7, None]

    rows = []

    for file_name, g in get_graphs(directory_path):
        g_len = len(g)
        for threshold in thresholds:
            start_time = time()
            cg, mg, i = create_bschema(g, iterations=20, similarity_threshold=threshold, use_original_names = False)
            end_time = time()

            if threshold is not None:
                threshold_path = f'threshold-{int(threshold*100)}/'
            else:
                threshold_path = 'full/'

            bschema_file_name = 'bschema/' + threshold_path + file_name
            os.makedirs(os.path.dirname(bschema_file_name), exist_ok=True)
            bind_prefixes(cg)
            cg.serialize(bschema_file_name, format="turtle")

            member_file_name = 'bschema-members/' + threshold_path + file_name
            os.makedirs(os.path.dirname(member_file_name), exist_ok=True)
            bind_prefixes(mg)
            mg.serialize(member_file_name, format="turtle")

            print(f"File: {file_name}, Threshold: {threshold}, compressed to {len(cg)/g_len*100:.2f}% of its original size")

            rows.append([file_name, threshold, g_len, len(cg), i, end_time - start_time])

            del cg
            del mg

        del g
        gc.collect()

    os.makedirs('bschema', exist_ok=True)
    stats_path = 'bschema/stats.csv'
    with open(stats_path, 'w', newline='') as csvfile:
        writer = csv.writer(csvfile)
        writer.writerow(["file_name", "threshold", "graph_length", "bschema_length", "iterations", "runtime"])
        writer.writerows(rows)

    plot_stats(stats_path)
