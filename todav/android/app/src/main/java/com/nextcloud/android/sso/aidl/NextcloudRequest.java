/*
 * SPDX-FileCopyrightText: 2017-2024 Nextcloud GmbH and Nextcloud contributors
 * SPDX-FileCopyrightText: 2017 David Luhmer <david-dev@live.de>
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Adapted from Nextcloud's Android-SingleSignOn library: the request the Nextcloud app reads
 * back by Java serialization, so its name, fields and serialVersionUID must stay as they are.
 */
package com.nextcloud.android.sso.aidl;

import com.nextcloud.android.sso.QueryParam;

import java.io.Serializable;
import java.util.Collection;
import java.util.HashMap;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;

public class NextcloudRequest implements Serializable {
    private static final long serialVersionUID = 215521212534240L;

    private String method;
    private Map<String, List<String>> header = new HashMap<>();
    private Map<String, String> parameter = new HashMap<>();
    private String requestBody;
    private String url;
    private String token;
    private String packageName;
    private String accountName;
    private boolean followRedirects;
    private Collection<QueryParam> parameterV2 = new LinkedList<>();

    public NextcloudRequest(String method, String url, Map<String, List<String>> header, String requestBody,
                            Collection<QueryParam> parameters, String accountName, String token, String packageName) {
        this.method = method;
        this.url = url;
        this.header = header;
        this.requestBody = requestBody;
        this.parameterV2 = new LinkedList<>(parameters);
        for (QueryParam p : parameters) {
            this.parameter.put(p.key, p.value);
        }
        this.accountName = accountName;
        this.token = token;
        this.packageName = packageName;
    }
}
