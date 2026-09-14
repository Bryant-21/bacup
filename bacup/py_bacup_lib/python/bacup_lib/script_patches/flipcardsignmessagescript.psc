Function UpdateMessage()
	; FO76 Utility.SplitStringChars() has no Fallout 4 equivalent, and vanilla FO4
	; Papyrus offers no string indexing/length at all, so MessageToDisplay cannot be
	; decomposed into per-card characters. BEHAVIOUR LOST: dynamic (runtime-set) sign
	; text. The authored Message0 card list is displayed instead.
	If Message0 != None && Message0.Length > 0
		Self.DisplayMessage(Message0)
	EndIf
EndFunction

; OnSyncVariableNetworkChanged replicated MessageToDisplay to clients; nothing
; replicates in single-player and UpdateMessage() remains directly callable.
; @drop-member OnSyncVariableNetworkChanged
