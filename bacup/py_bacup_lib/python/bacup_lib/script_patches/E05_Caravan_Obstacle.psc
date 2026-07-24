Event OnDestructionStageChanged(int aiOldStage, int aiCurrentStage)
    If GetState() != "destroyed" && Self.IsDestroyed()
        GoToState("destroyed")
        Self.PlaceAtMe(Explosive)
        Self.PlaceAtMe(Debris)
        Utility.Wait(DisableDelay)
        Self.DisableNoWait()
    EndIf
EndEvent
