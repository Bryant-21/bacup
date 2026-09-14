Event OnActivate(ObjectReference akActionRef)
	; FO76 IsAPlayer() -> single-player identity test. FO76's asynchronous
	; RegisterForMessageBoxPressEvent + Message.ShowSingle(receiver, ...) pair
	; collapses to FO4's synchronous Message.Show(). NOTE: the converted script has
	; no surviving handler for the button result, so the choice is still discarded.
	If akActionRef == Game.GetPlayer()
		RepairMeMessage.Show()
	EndIf
EndEvent
