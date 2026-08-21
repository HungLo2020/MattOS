/* t-cond.c - Test condition variable
 * Copyright 2024 g10 Code GmbH
 *
 * This file is free software; as a special exception the author gives
 * unlimited permission to copy and/or distribute it, with or without
 * modifications, as long as this notice is preserved.
 *
 * This file is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY, to the extent permitted by law; without even the
 * implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 *
 * You can also use this file under the licence of CC0.
 * SPDX-License-Identifier: CC0
 */

#include "t-support.h"

static npth_mutex_t mutex;
static npth_cond_t cond;
static int counter_waiting;

#define MAX_THREAD 17 /* Cerebrating Shohei's 50/50 accomplishment in 2024.  */

static void *
fn_testing_cond_signal (void *arg)
{
  int rc;
  int *thread_number = arg;
  char msg[256];
  int signal_invoked = 0;

  snprintf (msg, DIM (msg), "thread %d: start", *thread_number);
  info_msg (msg);

  rc = npth_mutex_lock (&mutex);
  fail_if_err (rc);

  if (counter_waiting < MAX_THREAD - 1)
    {
      snprintf (msg, DIM (msg), "thread %d: wait", *thread_number);
      info_msg (msg);
      counter_waiting++;
      rc = npth_cond_wait (&cond, &mutex);
      fail_if_err (rc);
      counter_waiting--;
    }
  else
    {
      rc = npth_mutex_unlock (&mutex);
      fail_if_err (rc);

    again:
      if (++signal_invoked > MAX_THREAD * 10)
        fail_msg ("Many cond_signal invoked, but no success.");

      rc = npth_mutex_lock (&mutex);
      fail_if_err (rc);
      if (counter_waiting)
        {
          snprintf (msg, DIM (msg), "thread %d: signal: %d", *thread_number,
                    counter_waiting);
          info_msg (msg);
          rc = npth_cond_signal (&cond);
          fail_if_err (rc);

          rc = npth_mutex_unlock (&mutex);
          fail_if_err (rc);

          npth_usleep (10);  /* Give the other threads some time to run.  */
          goto again;
        }
    }

  rc = npth_mutex_unlock (&mutex);
  fail_if_err (rc);

  snprintf (msg, DIM (msg), "thread %d: done", *thread_number);
  info_msg (msg);

  return NULL;
}


static void *
fn_testing_cond_broadcast (void *arg)
{
  int rc;
  int *thread_number = arg;
  char msg[256];

  snprintf (msg, DIM (msg), "thread %d: start", *thread_number);
  info_msg (msg);

  rc = npth_mutex_lock (&mutex);
  fail_if_err (rc);

  if (counter_waiting < MAX_THREAD - 1)
    {
      snprintf (msg, DIM (msg), "thread %d: wait", *thread_number);
      info_msg (msg);
      counter_waiting++;
      rc = npth_cond_wait (&cond, &mutex);
      fail_if_err (rc);
      counter_waiting--;
    }
  else
    {
      snprintf (msg, DIM (msg), "thread %d: broadcast: %d", *thread_number,
                counter_waiting);
      info_msg (msg);
      rc = npth_cond_broadcast (&cond);
      fail_if_err (rc);
    }

  rc = npth_mutex_unlock (&mutex);
  fail_if_err (rc);

  snprintf (msg, DIM (msg), "thread %d: done", *thread_number);
  info_msg (msg);

  return NULL;
}


static void
run_test_with_fn (npth_attr_t tattr, void * (*func)(void *arg))
{
  int rc;
  int i;
  int id[MAX_THREAD];
  npth_t tid[MAX_THREAD];
  void *retval;

  info_msg ("creating threads");
  for (i = 0; i < MAX_THREAD; i++)
    {
      id[i] = i;
      rc = npth_create (&tid[i], &tattr, func, &id[i]);
      fail_if_err (rc);
    }

  for (i = 0; i < MAX_THREAD; i++)
    {
      rc = npth_join (tid[i], &retval);
      fail_if_err (rc);
    }
  info_msg ("joined threads");
}


int
main (int argc, char *argv[])
{
  int rc;
  npth_attr_t tattr;

  if (argc >= 2 && !strcmp (argv[1], "--verbose"))
    opt_verbose = 1;

  rc = npth_init ();
  fail_if_err (rc);

  rc = npth_mutex_init (&mutex, NULL);
  fail_if_err (rc);
  info_msg ("mutex initialized");

  rc = npth_cond_init (&cond, NULL);
  fail_if_err (rc);
  info_msg ("cond initialized");

  rc = npth_attr_init (&tattr);
  fail_if_err (rc);
  rc = npth_attr_setdetachstate (&tattr, NPTH_CREATE_JOINABLE);
  fail_if_err (rc);

  run_test_with_fn (tattr, fn_testing_cond_signal);
  run_test_with_fn (tattr, fn_testing_cond_broadcast);

  rc = npth_attr_destroy (&tattr);
  fail_if_err (rc);

  rc = npth_mutex_destroy (&mutex);
  fail_if_err (rc);
  info_msg ("mutex destroyed");

  rc = npth_cond_destroy (&cond);
  fail_if_err (rc);
  info_msg ("cond destroyed");

  return 0;
}
