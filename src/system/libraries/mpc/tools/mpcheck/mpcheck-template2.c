/* template for complex -> real function, for example abs

Copyright (C) 2020 INRIA

This file is part of GNU MPC.

GNU MPC is free software; you can redistribute it and/or modify it under
the terms of the GNU Lesser General Public License as published by the
Free Software Foundation; either version 3 of the License, or (at your
option) any later version.

GNU MPC is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
FOR A PARTICULAR PURPOSE. See the GNU Lesser General Public License for
more details.

You should have received a copy of the GNU Lesser General Public License
along with this program. If not, see http://www.gnu.org/licenses/ .
*/

#define CAT1(X,Y) X ## Y
#define CAT2(X,Y) CAT1(X,Y)
#define STRINGIFY(x) #x
#define TOSTRING(x) STRINGIFY(x)

#define FUN CAT2(test_,FOO)
#define CHECK CAT2(check_,FOO)
#define MPC_FOO CAT2(mpc_,FOO)
#define CFOO CAT2(CAT2(c,FOO),SUFFIX)
#define BAR TOSTRING(FOO)

#define EXTRA 20

/* routine to double-check result from MPC with more precision and without
   reduced exponent range */
static void
CHECK (mpc_t x, mpfr_t z)
{
  mpfr_exp_t saved_emin = mpfr_get_emin ();
  mpfr_exp_t saved_emax = mpfr_get_emax ();
  mpfr_prec_t p = mpfr_get_prec (mpc_realref (x)), pp = p + EXTRA;
  mpfr_t t;
  int inex, ok;

  /* enlarge exponent range to the largest possible */
  mpfr_set_emin (mpfr_get_emin_min ());
  mpfr_set_emax (mpfr_get_emax_max ());

  mpfr_init2 (t, pp);
  MPC_FOO (t, x, MPFR_RNDN);
  inex = mpfr_prec_round (t, p, MPFR_RNDN);
  /* restore exponent range */
  mpfr_set_emin (saved_emin);
  mpfr_set_emax (saved_emax);
  inex = mpfr_check_range (t, inex, MPFR_RNDN);
  inex = mpfr_subnormalize (t, inex, MPFR_RNDN);

  /* check real parts agree */
  ok = mpfr_agree (z, t, inex);

  if (!ok)
    {
      mpfr_printf ("Potential bug in mpc_%s for x=(%Re,%Re)\n", BAR,
                   mpc_realref (x), mpc_imagref (x));
      mpfr_printf ("   mpc_%s to precision %lu gives %Re\n", BAR, p, z);
      mpfr_printf ("   mpc_%s to precision %lu gives %Re\n", BAR, pp, t);
      exit (1);
    }

  mpfr_clear (t);
}

static void
FUN (mpfr_prec_t p, unsigned long n)
{
  unsigned long i = 0;
  mpc_t x;
  mpfr_t z, t;
  TYPE complex xx;
  TYPE zz;
  int inex;
  unsigned long errors = 0, max_err = 0;

  if (verbose > 0)
    printf ("Testing function %s\n", BAR);

  gmp_randseed_ui (state, seed);

  mpc_init2 (x, p);
  mpfr_init2 (z, p);
  mpfr_init2 (t, p);
  for (i = 0; i < n; i++)
    {
      mpcheck_random (x);
      inex = MPC_FOO (z, x, MPC_RNDNN);
      mpfr_subnormalize (z, inex, MPFR_RNDN);
      xx = mpc_get_type (x, MPC_RNDNN);
      zz = CFOO (xx);
      mpfr_set_type (t, zz, MPFR_RNDN);
      if (mpcheck_mpfr_cmp (t, z) != 0)
        {
          unsigned long err = ulp_error (t, z);
          if (verbose > 0 && err > max_err)
            {
              mpfr_printf ("   mpc_%s and c%s differ by %lu ulp(s)\n      for x=(%Re,%Re)\n",
                           BAR, BAR, err,
                           mpc_realref (x), mpc_imagref (x));
              mpfr_printf ("   mpc_%s gives %Re\n", BAR, z);
              mpfr_printf ("      c%s gives %Re\n", BAR, t);
            }
          if (recheck)
            CHECK (x, z);
          errors++;
          if (err > max_err)
            {
              max_err = err;
              if (max_err > Max_err)
                Max_err = max_err;
            }
        }
    }
  mpc_clear (x);
  mpfr_clear (z);
  mpfr_clear (t);
  printf ("Errors for %s: %lu/%lu (max %lu) [seed %lu]\n",
          BAR, errors, n, max_err, seed);
}

#undef FOO
#undef CFOO
