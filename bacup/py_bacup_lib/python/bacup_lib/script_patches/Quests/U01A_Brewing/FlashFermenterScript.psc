; The speakeasy Flash-Fermenter turns the player's Fermenting Nukashine into Fresh Nukashine once, then bursts
; (FlashFermenterRef.DestroySelf swaps in the destroyed model).
Event OnAliasInit()
    GoToState("ready")
EndEvent

State ready
    Event OnActivate(ObjectReference akActionRef)
        Actor player = Game.GetPlayer()
        If akActionRef != player || NukashineFerm == None || NukashineFresh == None
            Return
        EndIf
        Int count = player.GetItemCount(NukashineFerm)
        If count <= 0
            Return
        EndIf
        GoToState("broken")
        player.RemoveItem(NukashineFerm, count)
        player.AddItem(NukashineFresh, count)
        Quests:U01A_Brewing:FlashFermenterRef fermenter = GetReference() as Quests:U01A_Brewing:FlashFermenterRef
        If fermenter != None
            If fermenter.P01A_Nukashine_UsedFlashFermenter != None
                player.SetValue(fermenter.P01A_Nukashine_UsedFlashFermenter, 1.0)
            EndIf
            fermenter.DestroySelf(True)
        EndIf
        Quest owner = GetOwningQuest()
        If owner != None && !owner.IsStageDone(Stage_UsedFlashFermenter)
            owner.SetStage(Stage_UsedFlashFermenter)
        EndIf
    EndEvent
EndState

State broken
EndState
