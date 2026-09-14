Event OnActivate(ObjectReference akActionRef)
	Actor activatingActor = akActionRef as Actor
	If activatingActor != None && ActorValueBoolToSet != None
		Float currentValue = activatingActor.GetValue(ActorValueBoolToSet)
		If currentValue > 0.0
			activatingActor.SetValue(ActorValueBoolToSet, 0.0)
		Else
			activatingActor.SetValue(ActorValueBoolToSet, 1.0)
		EndIf
	EndIf
EndEvent
