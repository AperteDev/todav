/*
 * SPDX-FileCopyrightText: 2017-2024 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * Adapted from Nextcloud's Android-SingleSignOn library (serialized as part of NextcloudRequest).
 */
package com.nextcloud.android.sso;

import java.io.Serializable;

public class QueryParam implements Serializable {
    private static final long serialVersionUID = 21523240203234211L;

    public String key;
    public String value;

    public QueryParam(String key, String value) {
        this.key = key;
        this.value = value;
    }
}
