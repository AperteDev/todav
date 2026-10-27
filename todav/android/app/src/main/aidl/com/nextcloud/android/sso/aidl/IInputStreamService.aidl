/*
 * SPDX-FileCopyrightText: 2017-2024 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * The Nextcloud app's Single Sign-On service, as Nextcloud's Android-SingleSignOn library declares it.
 */
package com.nextcloud.android.sso.aidl;

interface IInputStreamService {
    ParcelFileDescriptor performNextcloudRequestAndBodyStream(in ParcelFileDescriptor input,
                                                              in ParcelFileDescriptor requestBodyParcelFileDescriptor);
    ParcelFileDescriptor performNextcloudRequest(in ParcelFileDescriptor input);

    ParcelFileDescriptor performNextcloudRequestAndBodyStreamV2(in ParcelFileDescriptor input,
                                                                in ParcelFileDescriptor requestBodyParcelFileDescriptor);
    ParcelFileDescriptor performNextcloudRequestV2(in ParcelFileDescriptor input);
}
