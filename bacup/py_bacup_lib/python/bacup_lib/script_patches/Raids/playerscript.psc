Event OnMenuOpenCloseEvent(String asMenuName, Bool abOpening)
	If !abOpening
		Self.UnregisterForMenuOpenCloseEvent("UniversalRewardsMenu")
		; FO76 SendRMIToServer("RemovePlayerFromRaid") -> direct local call on the
		; actor this script is attached to.
		Self.RemovePlayerFromRaid(Self)
	EndIf
EndEvent
