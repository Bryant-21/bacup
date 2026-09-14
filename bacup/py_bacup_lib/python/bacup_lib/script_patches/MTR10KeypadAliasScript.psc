Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer()
		Return
	EndIf
	Form keyForm = None
	If KeyCardAlias != None && KeyCardAlias.GetReference() != None
		keyForm = KeyCardAlias.GetReference().GetBaseObject()
	EndIf
	If keyForm != None && playerRef.GetItemCount(keyForm) > 0
		If KeypadsActiveGlobal != None
			KeypadsActiveGlobal.SetValue(1.0)
		EndIf
		If MyKeypadGlobal != None
			MyKeypadGlobal.SetValue(1.0)
		EndIf
		If OtherKeypadGlobal != None
			OtherKeypadGlobal.SetValue(0.0)
		EndIf
		If SuccessMessage != None
			SuccessMessage.Show()
		EndIf
	ElseIf FailMessage != None
		FailMessage.Show()
	EndIf
EndEvent
