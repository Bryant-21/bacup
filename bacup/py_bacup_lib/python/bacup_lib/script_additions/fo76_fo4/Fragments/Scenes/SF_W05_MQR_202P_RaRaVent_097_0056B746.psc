Scriptname Fragments:Scenes:SF_W05_MQR_202P_RaRaVent_097_0056B746 Extends Scene Hidden Const

; FO76 never shipped this fragment to the client, so conversion has no skeleton
; to patch and strips the scene's VMAD. Phase 2 of W05_MQR_202P_RaRaVent_0970_Enter_0980_Exit
; completes only once alias 24 (RaRaVent0970Enter) is enabled, and that furniture
; starts disabled; without this VentSwap callback the scene never advances.
Function Fragment_Phase_01_End()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.EnableRaRaEntryVent(controller.RaRaVent0970Enter)
    EndIf
EndFunction
