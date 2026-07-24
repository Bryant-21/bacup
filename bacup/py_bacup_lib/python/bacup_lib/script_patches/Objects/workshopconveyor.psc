Event OnPowerOff()
	Self.TurnConveyorOn(False)
EndEvent

Event OnPowerOn(ObjectReference akPowerGenerator)
	Self.TurnConveyorOn(True)
EndEvent
