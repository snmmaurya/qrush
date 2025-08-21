{% extends "layout.html.tera" %}

{% block title %}Queues{% endblock title %}

{% block content %}
  <div class="bg-white shadow rounded-lg p-6 dark:bg-gray-900 dark:text-white">
    <h2 class="text-xl font-semibold mb-4">Available Queues</h2>

    <!-- Summary + Filter -->
    <div class="flex flex-col sm:flex-row sm:justify-between sm:items-center mb-4 gap-4">
      <div class="flex flex-wrap gap-2">
        <div class="text-sm text-white bg-green-600 px-3 py-1 rounded-lg">
          Success Jobs: <strong>{{ stats.success_jobs }}</strong>
        </div>
        <div class="text-sm text-white bg-red-600 px-3 py-1 rounded-lg">
          Failed Jobs: <strong>{{ stats.failed_jobs }}</strong>
        </div>
      </div>

      <form method="GET" class="flex gap-2 items-center">
        <input
          type="text"
          name="search"
          placeholder="Search queues"
          value="{{ query.search | default(value="") }}"
          class="border border-gray-300 dark:border-gray-700 rounded px-3 py-2 bg-white dark:bg-gray-800 text-sm text-gray-900 dark:text-gray-100 placeholder-gray-400 dark:placeholder-gray-500"
        />
        <button type="submit" class="bg-blue-600 hover:bg-blue-700 text-white px-4 py-2 rounded-full text-sm font-medium transition">
          Filter
        </button>
      </form>
    </div>

    {% if queues | length == 0 %}
      <p class="text-gray-400">No queues found.</p>
    {% else %}
      <div class="overflow-x-auto">
        <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700 text-sm">
          <thead class="bg-gray-100 dark:bg-gray-800 text-gray-700 dark:text-gray-300 font-semibold">
            <tr>
              <th class="px-4 py-2 text-left">Queue</th>
              <th class="px-4 py-2 text-left">Pending</th>
              <th class="px-4 py-2 text-left">Retry</th>
              <th class="px-4 py-2 text-left">Success</th>
              <th class="px-4 py-2 text-left">Failed</th>
              <th class="px-4 py-2 text-left">Actions</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-gray-100 dark:divide-gray-800">
            {% for queue in queues %}
              <tr class="hover:bg-gray-50 dark:hover:bg-gray-800 text-gray-800 dark:text-gray-100">
                <td class="px-4 py-2 font-mono">{{ queue.name }}</td>
                <td class="px-4 py-2">{{ queue.pending }}</td>
                <td class="px-4 py-2">{{ queue.retry }}</td>
                <td class="px-4 py-2 text-green-600 dark:text-green-400">{{ queue.success }}</td>
                <td class="px-4 py-2 text-red-600 dark:text-red-400">{{ queue.failed }}</td>
                <td class="px-4 py-2">
                  <a href="/qrush/metrics/queues/{{ queue.name }}"
                    class="text-sm text-white bg-blue-600 hover:bg-blue-700 px-3 py-1 rounded-full transition">
                    View Jobs
                  </a>
                </td>
              </tr>
            {% endfor %}
          </tbody>
        </table>
      </div>
    {% endif %}

    {% if page is defined %}
      <div class="flex flex-col sm:flex-row justify-between items-center mt-4 text-sm text-gray-600 dark:text-gray-300 gap-2">
        <div>
          Showing {{ page.start }} to {{ page.end }} of {{ page.total }} queues
        </div>
        <div class="space-x-2">
          {% if page.has_prev %}
            <a href="?page={{ page.current - 1 }}&{{ page.query }}" class="px-3 py-1 border rounded dark:text-white dark:border-gray-600">
              Prev
            </a>
          {% endif %}
          {% if page.has_next %}
            <a href="?page={{ page.current + 1 }}&{{ page.query }}" class="px-3 py-1 border rounded dark:text-white dark:border-gray-600">
              Next
            </a>
          {% endif %}
        </div>
      </div>
    {% endif %}
  </div>
{% endblock content %}
