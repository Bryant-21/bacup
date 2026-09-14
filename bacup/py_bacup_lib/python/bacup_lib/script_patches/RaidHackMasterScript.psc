Event OnLoad()
	; The ScriptObject casts made the compiler resolve OnTriggerEnter on ScriptObject.
	Self.RegisterForRemoteEvent(Self.GetLinkedRef(LinkCustom01), "OnTriggerEnter")
	Self.RegisterForRemoteEvent(Self.GetLinkedRef(LinkCustom02), "OnTriggerEnter")
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If akSender == Self.GetLinkedRef(LinkCustom01)
		Self.ClientHasJoinedAttackers(akActionRef)
	ElseIf akSender == Self.GetLinkedRef(LinkCustom02)
		Self.ClientHasJoinedDefenders(akActionRef)
	EndIf
EndEvent

; Re-homed from OnSyncVariableNetworkChanged("DefenderCount"/"AttackerCount"):
; FO76 replicated the team tallies and clients showed the notification. Single-player
; has no replication, so the notification is exposed as a local entry point.
Function NotifyTeamStatusChanged()
	Self.TeamStatusNotification(AttackerCount, DefenderCount)
EndFunction

; @drop-member OnSyncVariableNetworkChanged
