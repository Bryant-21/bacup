Event OnActivate(ObjectReference akActionRef)
	; FO76 IsLocalPlayer() -> single-player identity test. NOTE: the innermost
	; body is empty in the converted (server-stripped) script.
	If akActionRef == Game.GetPlayer() && akActionRef.GetValue(ActorValueToSet) != ValueToSet
		If KeywordToCheck == None || akActionRef.HasKeyword(KeywordToCheck)
		EndIf
	EndIf
EndEvent
