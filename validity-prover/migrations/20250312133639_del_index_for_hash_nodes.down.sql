CREATE INDEX idx_hash_nodes_timestamp ON public.hash_nodes USING btree (timestamp_value DESC, tag);
