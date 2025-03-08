CREATE INDEX idx_leaves_lookup ON public.leaves USING btree ("position", tag, timestamp_value DESC);
CREATE INDEX idx_leaves_timestamp ON public.leaves USING btree (timestamp_value DESC, tag);
