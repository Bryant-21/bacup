; AmbushRelease > 0 means the owning encounter has released (or never wants) the ambush pose;
; Enc06 sets it to 1 at stage 100 and relies on this script staying out of the way.
Event OnLoad()
	If IsDisabled() || IsDead() || GetValue(AmbushRelease) > 0.0
		Return
	EndIf
	If !WaitFor3DLoad()
		Return
	EndIf
	If IsDisabled() || IsDead() || GetValue(AmbushRelease) > 0.0
		Return
	EndIf
	ObjectReference ambushFurniture = GetLinkedRef(LinkAmbushFurniture)
	If ambushFurniture != None
		Bool snapped = SnapIntoInteraction(ambushFurniture)
		Debug.Trace("[B21 Raids] ambush snap " + Self + " succeeded=" + snapped)
	EndIf
EndEvent
