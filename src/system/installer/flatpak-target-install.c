/*
 * MattOS installer helper for optional Flatpak applications.
 *
 * This intentionally runs in the booted live environment, where DNS, TLS,
 * GnuPG/GPGME, and OSTree have their normal runtime state.  It opens the
 * mounted installation's system Flatpak directory explicitly, so downloaded
 * objects and deployments are written only below the target root.  In
 * particular, this is not a chrooted invocation of the target's flatpak CLI.
 */

#include <flatpak/flatpak.h>
#include <glib.h>
#include <stdio.h>
#include <unistd.h>

static void
usage (const char *program)
{
  g_printerr ("Usage: %s --target-root PATH --remote NAME --app APP_ID [--check-target]\n",
              program);
}

static void
transaction_new_operation (FlatpakTransaction          *transaction,
                           FlatpakTransactionOperation *operation,
                           FlatpakTransactionProgress  *progress,
                           gpointer                     user_data)
{
  (void) transaction;
  (void) progress;
  (void) user_data;
  g_print ("flatpak-target-install: %s %s\n",
           flatpak_transaction_operation_type_to_string (
             flatpak_transaction_operation_get_operation_type (operation)),
           flatpak_transaction_operation_get_ref (operation));
}

int
main (int argc, char **argv)
{
  const char *target_root = NULL;
  const char *remote = NULL;
  const char *app_id = NULL;
  gboolean check_target = FALSE;
  g_autofree char *installation_path = NULL;
  g_autoptr (GFile) installation_file = NULL;
  g_autoptr (FlatpakInstallation) installation = NULL;
  g_autoptr (FlatpakRemote) configured_remote = NULL;
  g_autoptr (FlatpakInstalledRef) installed = NULL;
  g_autoptr (FlatpakTransaction) transaction = NULL;
  g_autoptr (GError) error = NULL;
  g_autofree char *ref = NULL;

  for (int index = 1; index < argc; index++)
    {
      if (g_str_equal (argv[index], "--target-root") && index + 1 < argc)
        target_root = argv[++index];
      else if (g_str_equal (argv[index], "--remote") && index + 1 < argc)
        remote = argv[++index];
      else if (g_str_equal (argv[index], "--app") && index + 1 < argc)
        app_id = argv[++index];
      else if (g_str_equal (argv[index], "--check-target"))
        check_target = TRUE;
      else
        {
          usage (argv[0]);
          return 2;
        }
    }

  if (target_root == NULL || remote == NULL || (app_id == NULL && !check_target))
    {
      usage (argv[0]);
      return 2;
    }
  if (geteuid () != 0 && !check_target)
    {
      g_printerr ("flatpak-target-install: must run as root\n");
      return 2;
    }
  if (!g_path_is_absolute (target_root))
    {
      g_printerr ("flatpak-target-install: target root must be absolute: %s\n", target_root);
      return 2;
    }

  installation_path = g_build_filename (target_root, "var", "lib", "flatpak", NULL);
  installation_file = g_file_new_for_path (installation_path);
  installation = flatpak_installation_new_for_path (installation_file, FALSE, NULL, &error);
  if (installation == NULL)
    {
      g_printerr ("flatpak-target-install: open target installation %s: %s\n",
                  installation_path, error->message);
      return 1;
    }
  flatpak_installation_set_no_interaction (installation, TRUE);

  configured_remote = flatpak_installation_get_remote_by_name (installation, remote, NULL, &error);
  if (configured_remote == NULL)
    {
      g_printerr ("flatpak-target-install: target remote %s: %s\n", remote, error->message);
      return 1;
    }

  g_print ("flatpak-target-install: target=%s remote=%s\n", installation_path, remote);
  if (check_target)
    return 0;

  installed = flatpak_installation_get_installed_ref (installation,
                                                       FLATPAK_REF_KIND_APP,
                                                       app_id,
                                                       flatpak_get_default_arch (),
                                                       "stable",
                                                       NULL,
                                                       NULL);
  if (installed != NULL)
    {
      g_print ("flatpak-target-install: %s is already installed in target\n", app_id);
      return 0;
    }

  ref = g_strdup_printf ("app/%s/%s/stable", app_id, flatpak_get_default_arch ());
  transaction = flatpak_transaction_new_for_installation (installation, NULL, &error);
  if (transaction == NULL)
    {
      g_printerr ("flatpak-target-install: prepare transaction for %s in %s: %s\n",
                  app_id, installation_path, error->message);
      return 1;
    }
  flatpak_transaction_set_no_interaction (transaction, TRUE);
  if (!flatpak_transaction_add_install (transaction, remote, ref, NULL, &error))
    {
      g_printerr ("flatpak-target-install: add %s from %s into %s: %s\n",
                  app_id, remote, installation_path, error->message);
      return 1;
    }
  g_signal_connect (transaction, "new-operation", G_CALLBACK (transaction_new_operation), NULL);
  if (!flatpak_transaction_run (transaction, NULL, &error))
    {
      g_printerr ("flatpak-target-install: install %s from %s into %s: %s\n",
                  app_id, remote, installation_path, error->message);
      return 1;
    }

  g_print ("flatpak-target-install: installed %s from %s into %s\n",
           app_id, remote, installation_path);
  return 0;
}
