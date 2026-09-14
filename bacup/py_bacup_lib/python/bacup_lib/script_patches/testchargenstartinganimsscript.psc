Event OnLoad()
	If SingleFurnitureActivate == False
		; The ScriptObject casts made the compiler resolve OnSit/OnGetUp on ScriptObject.
		Self.RegisterForRemoteEvent(game.GetPlayer(), "OnSit")
		Self.RegisterForRemoteEvent(game.GetPlayer(), "OnGetUp")
	EndIf
EndEvent

Event actor.OnSit(actor akSender, ObjectReference akFurniture)
	If akFurniture == WakeUpFurniture
		utility.Wait(1.0)
		akFurniture.Activate(akSender as ObjectReference, False)
	EndIf
	If akFurniture == FaceGenFurniture
		; FO76 Message.ShowSingle(receiver, ...) -> FO4 Message.Show().
		TestCharGenStartingAnimMessage.Show()
	EndIf
	If akFurniture == TakePipboyFurniture
		utility.Wait(1.0)
		akFurniture.Activate(akSender as ObjectReference, False)
	EndIf
EndEvent
