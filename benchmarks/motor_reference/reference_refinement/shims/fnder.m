function dpp = fnder (pp)
  % Octave stand-in for the Curve Fitting Toolbox fnder, first derivative
  % of a piecewise polynomial only (the single use in the ICEM snapshot:
  % Mat_Magnetic.fitHBspline). Written for GEODE issue #1189.
  dpp = ppder (pp);
end
