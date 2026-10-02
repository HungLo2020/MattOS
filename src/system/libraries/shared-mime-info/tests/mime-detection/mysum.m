% MYAVG(X) Calculates the average of the elements
function m=myavg(x)
n=length(x);
mysum=0;
for i=1:n
  mysum=mysum+x(i)
end
m=mysum/n;
