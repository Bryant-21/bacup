; Alias 20 finds Aldridge by location ref type, but this reference is disabled
; until its instance-swap enable ref turns on at 1300, so the alias is empty
; when the quest starts. Push this reference into the controller's alias once it
; loads; the fill can re-disable it if the alias is initially disabled.
Event OnLoad()
    If W05_MQ_101P_A == None || !W05_MQ_101P_A.IsRunning()
        Return
    EndIf
    W05_MQ_101P_A_QuestScript controller = W05_MQ_101P_A as W05_MQ_101P_A_QuestScript
    If controller == None || controller.Aldridge == None
        Return
    EndIf
    If controller.Aldridge.GetReference() != Self
        controller.Aldridge.ForceRefTo(Self)
    EndIf
    If IsDisabled()
        Enable()
    EndIf
    Actor selfActor = (Self as ObjectReference) as Actor
    If selfActor
        selfActor.EvaluatePackage()
    EndIf
EndEvent
